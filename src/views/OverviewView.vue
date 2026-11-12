<script setup lang="ts">
import { computed } from "vue";
import { artifacts, counts, goTo, primaryReason, selectedSummary, snapshot, worktrees } from "../state";
import SafetyBadge from "../components/SafetyBadge.vue";
import ReasonList from "../components/ReasonList.vue";
import StatCard from "../components/StatCard.vue";
import { basename, compareSizeNullsFirst, formatBytes, formatCount, formatRelTime, displayPath, isUnder } from "../format";
import { messages } from "../i18n";

const root = computed(() => snapshot.value?.root ?? "");

/**
 * Whether a worktree lives outside the directory the user picked.
 *
 * A worktree of the chosen repository can be anywhere — `git worktree add` takes any
 * path — so a row in a "largest worktrees" list may name a directory the reader has
 * never seen. Telling, not deciding: the verdict comes from Git.
 */
function isOutsideRoot(path: string): boolean {
  return root.value.length > 0 && !isUnder(path, root.value);
}

/**
 * The main workspace is not a deletion candidate. It is the repository itself;
 * the merge check is anchored to the default branch (origin/main), not to the
 * branch this workspace happens to have checked out. It is shown as a protected
 * strip rather than as a row in the "largest worktrees" list.
 */
const mainWorktree = computed(() => worktrees.value.find((item) => item.is_main) ?? null);

/** The remote-tracking ref the merge check actually uses, for the hint copy. */
const defaultBranchRef = computed(() => {
  const branch = snapshot.value?.default_branch;
  if (!branch) return "";
  return branch.startsWith("origin/") ? branch : `origin/${branch}`;
});

// Unknown sizes sort first: an item whose size could not be measured must not be
// buried at the bottom of a "largest" list (see compareSizeNullsFirst).
const topWorktrees = computed(() =>
  [...worktrees.value]
    .filter((item) => !item.is_main)
    .sort((a, b) => compareSizeNullsFirst(a.size_bytes, b.size_bytes))
    .slice(0, 5)
);

const topArtifacts = computed(() =>
  [...artifacts.value].sort((a, b) => compareSizeNullsFirst(a.size_bytes, b.size_bytes)).slice(0, 5)
);

const safeBytes = computed(() => {
  const items = [
    ...worktrees.value.filter((w) => w.safety.status === "Safe"),
    ...artifacts.value.filter((a) => a.safety.status === "Safe")
  ];
  return {
    bytes: items.reduce((sum, item) => sum + (item.size_bytes ?? 0), 0),
    unknown: items.filter((item) => item.size_bytes === null).length
  };
});

const reviewCount = computed(() => counts.value.worktree.Review + counts.value.artifact.Review);
const unsafeCount = computed(() => counts.value.worktree.Unsafe + counts.value.artifact.Unsafe);
const selectionCount = computed(() => selectedSummary.value.worktrees + selectedSummary.value.artifacts);
</script>

<template>
  <div class="stack page--wide">
    <div class="spread">
      <div>
        <h1>{{ messages.overview.title }}</h1>
        <p class="faint tiny mono">{{ messages.overview.subtitle(defaultBranchRef || messages.common.unknown) }}</p>
      </div>
    </div>

    <div class="stat-grid stat-grid--4">
      <StatCard
        :label="messages.overview.stats.safeLabel"
        tone="safe"
        value-tone
        :value="String(counts.worktree.Safe + counts.artifact.Safe)"
        :sub="
          safeBytes.unknown > 0
            ? messages.overview.stats.safeSubUnknown(formatBytes(safeBytes.bytes, { approximate: true }), safeBytes.unknown)
            : messages.overview.stats.safeSub(formatBytes(safeBytes.bytes))
        "
      />
      <StatCard
        :label="messages.overview.stats.reviewLabel"
        tone="review"
        value-tone
        :value="String(reviewCount)"
        :sub="messages.overview.stats.reviewSub"
      />
      <StatCard
        :label="messages.overview.stats.unsafeLabel"
        tone="unsafe"
        value-tone
        :value="String(unsafeCount)"
        :sub="messages.overview.stats.unsafeSub"
      />
      <StatCard
        :label="messages.overview.stats.reclaimableLabel"
        tone="accent"
        :value="formatBytes(selectedSummary.bytes, { approximate: selectedSummary.unknown > 0 })"
        :sub="messages.overview.stats.reclaimableSub(selectionCount, selectedSummary.worktrees, selectedSummary.artifacts)"
      />
      <p v-if="selectedSummary.unknown > 0" class="faint tiny">
        {{ messages.overview.selectedUnknown(selectedSummary.unknown) }}
      </p>
    </div>

    <div v-if="mainWorktree" class="main-workspace">
      <div class="main-workspace__head">
        <span class="main-workspace__name">{{ basename(mainWorktree.path) }}</span>
        <span class="chip chip--protected" :title="messages.worktrees.mainWorkspaceHint(defaultBranchRef)">{{ messages.worktrees.mainWorkspaceBadge }}</span>
        <span class="main-workspace__meta">
          {{ mainWorktree.branch || messages.common.detached }} · {{ formatBytes(mainWorktree.size_bytes) }} · {{ formatRelTime(mainWorktree.last_updated_at) }}
        </span>
      </div>
      <p class="main-workspace__note">{{ messages.worktrees.mainWorkspaceHint(defaultBranchRef) }}</p>
    </div>

    <div v-for="warning in snapshot?.warnings || []" :key="warning" class="banner banner--warn">
      <span>⚠</span>
      <span>
        <span class="banner__title">{{ messages.overview.warningTitle }}</span>{{ warning }}
      </span>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>{{ messages.overview.largestWorktrees }}</h2>
        <button class="btn btn--ghost" @click="goTo('worktrees')">{{ messages.overview.viewAll(worktrees.length) }}</button>
      </div>
      <div class="card__body card__body--flush">
        <!-- The same three lines as the worktree page, without the checkbox: this
             page is a way in, not a place to choose. -->
        <div v-for="item in topWorktrees" :key="item.path" class="item-row item-row--plain">
          <div class="item-row__main">
            <div class="item-row__head">
              <span class="item-row__name" :title="item.path">{{ displayPath(item.path, root) }}</span>
              <span v-if="item.agent" class="chip">{{ item.agent }}</span>
            </div>
            <div class="item-row__meta">
              <span>{{ item.branch || messages.common.detached }}</span>
              <span>· {{ formatBytes(item.size_bytes) }}</span>
              <span>· {{ formatRelTime(item.last_updated_at) }}</span>
              <span
                v-if="isOutsideRoot(item.path)"
                class="chip chip--outside"
                :title="messages.worktrees.outsideRootHint"
              >{{ messages.worktrees.outsideRoot }}</span>
            </div>
            <div class="item-row__foot">
              <div class="item-row__reasons">
                <ReasonList :reasons="item.safety.reasons" />
              </div>
            </div>
          </div>
          <div class="item-row__figures">
            <SafetyBadge :status="item.safety.status" />
          </div>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>{{ messages.overview.largestArtifacts }}</h2>
        <button class="btn btn--ghost" @click="goTo('artifacts')">{{ messages.overview.viewAll(artifacts.length) }}</button>
      </div>
      <div class="card__body card__body--flush">
        <div v-for="item in topArtifacts" :key="item.path" class="item-row item-row--plain">
          <div class="item-row__main">
            <div class="item-row__head">
              <span class="chip">{{ item.kind }}</span>
              <span class="item-row__name" :title="item.path">{{ displayPath(item.path, root) }}</span>
            </div>
            <div class="item-row__meta">
              <span>{{ messages.overview.artifactMeta(item.ecosystem, formatCount(item.file_count)) }}</span>
              <span>· {{ formatRelTime(item.last_updated_at) }}</span>
            </div>
            <div class="item-row__foot">
              <div class="item-row__reasons">
                <ReasonList :reasons="item.safety.reasons" />
              </div>
            </div>
          </div>
          <div class="item-row__figures">
            <span class="item-row__size">{{ formatBytes(item.size_bytes) }}</span>
            <SafetyBadge :status="item.safety.status" />
          </div>
        </div>
      </div>
    </div>

    <!-- The rules behind the three counts. They sit after the two lists: the
         overview's job is to start a cleanup, and the pages where a decision is
         made repeat the one line that governs it. -->
    <section class="rules">
      <div class="rules__head">
        <h2>{{ messages.rules.title }}</h2>
        <span class="faint tiny">{{ messages.rules.hint }}</span>
      </div>
      <div class="rules__grid">
        <div class="rule">
          <div class="rule__title">
            <span class="dot dot--safe" aria-hidden="true"></span>{{ messages.status.label.Safe }}
          </div>
          <p class="rule__body">{{ messages.rules.safeBody }}</p>
          <p class="rule__default">{{ messages.rules.defaultSelected }}</p>
        </div>
        <div class="rule">
          <div class="rule__title">
            <span class="dot dot--review" aria-hidden="true"></span>{{ messages.status.label.Review }}
          </div>
          <p class="rule__body">{{ messages.rules.reviewBody }}</p>
          <p class="rule__default">{{ messages.rules.defaultUnselected }}</p>
        </div>
        <div class="rule">
          <div class="rule__title">
            <span class="dot dot--unsafe" aria-hidden="true"></span>{{ messages.status.label.Unsafe }}
          </div>
          <p class="rule__body">{{ messages.rules.unsafeBody }}</p>
          <p class="rule__default">{{ messages.rules.defaultDisabled }}</p>
        </div>
      </div>
      <p class="rules__foot">{{ messages.rules.footer }}</p>
    </section>

    <!-- The two ways into the lists live here, at the bottom, not up in the header
         beside the title: the header scrolls away with the page, and the overview is
         a long one — a user who reached the end had no way back into either list
         except the sidebar. The bar stays put, so the way forward is always visible. -->
    <div class="actionbar">
      <div class="actionbar__side">
        <button class="btn" @click="goTo('worktrees')">{{ messages.overview.viewWorktrees }}</button>
        <button class="btn" @click="goTo('artifacts')">{{ messages.overview.viewArtifacts }}</button>
      </div>
      <span class="actionbar__summary" :class="selectionCount > 0 ? 'muted' : 'faint'">
        {{ " " + messages.overview.selected(selectionCount) }} <strong>{{ formatBytes(selectedSummary.bytes, { approximate: selectedSummary.unknown > 0 }) }}</strong>
        <span v-if="selectionCount === 0" class="faint tiny">{{ messages.overview.selectHint }}</span>
      </span>
      <div class="actionbar__side" style="margin-left: auto">
        <button class="btn btn--primary" :disabled="selectionCount === 0" @click="goTo('plan')">
          {{ messages.overview.generatePlan }}
        </button>
      </div>
    </div>
  </div>
</template>
