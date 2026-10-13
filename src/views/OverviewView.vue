<script setup lang="ts">
import { computed } from "vue";
import { artifacts, counts, goTo, primaryReason, selectedSummary, snapshot, worktrees } from "../state";
import SafetyBadge from "../components/SafetyBadge.vue";
import StatCard from "../components/StatCard.vue";
import { compareSizeNullsFirst, formatBytes, formatCount, formatRelTime, displayPath, isUnder } from "../format";
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

// Unknown sizes sort first: an item whose size could not be measured must not be
// buried at the bottom of a "largest" list (see compareSizeNullsFirst).
const topWorktrees = computed(() =>
  [...worktrees.value].sort((a, b) => compareSizeNullsFirst(a.size_bytes, b.size_bytes)).slice(0, 5)
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
  <div class="stack page--reading">
    <div class="spread">
      <div>
        <h1>{{ messages.overview.title }}</h1>
        <p class="faint tiny mono">{{ messages.overview.subtitle(root, snapshot?.default_branch || messages.common.unknown) }}</p>
      </div>
      <div class="row-wrap">
        <button class="btn" @click="goTo('worktrees')">{{ messages.overview.viewWorktrees }}</button>
        <button class="btn" @click="goTo('artifacts')">{{ messages.overview.viewArtifacts }}</button>
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
        <table class="table">
          <tbody>
            <tr v-for="item in topWorktrees" :key="item.path">
              <td>
                <div class="mono">{{ displayPath(item.path, root) }}</div>
                <div class="tiny faint">
                  {{ item.branch || messages.common.detached }}{{ item.is_main ? messages.overview.mainWorktreeSuffix : "" }}
                  <span
                    v-if="isOutsideRoot(item.path)"
                    class="chip chip--outside"
                    :title="messages.worktrees.outsideRootHint"
                  >{{ messages.worktrees.outsideRoot }}</span>
                </div>
              </td>
              <td class="nowrap">
                <span class="chip">{{ item.agent || messages.common.unknownAgent }}</span>
              </td>
              <td class="cell-size">
                <div class="numeric">{{ formatBytes(item.size_bytes) }}</div>
                <div class="tiny faint">{{ formatRelTime(item.last_updated_at) }}</div>
              </td>
              <td class="nowrap"><SafetyBadge :status="item.safety.status" /></td>
              <td class="cell-size tiny faint" style="max-width: 200px">
                {{ primaryReason(item.safety.reasons)?.text }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>{{ messages.overview.largestArtifacts }}</h2>
        <button class="btn btn--ghost" @click="goTo('artifacts')">{{ messages.overview.viewAll(artifacts.length) }}</button>
      </div>
      <div class="card__body card__body--flush">
        <table class="table">
          <tbody>
            <tr v-for="item in topArtifacts" :key="item.path">
              <td>
                <div class="row-wrap">
                  <span class="chip">{{ item.kind }}</span>
                  <span class="mono">{{ displayPath(item.path, root) }}</span>
                </div>
                <div class="tiny faint">{{ messages.overview.artifactMeta(item.ecosystem, formatCount(item.file_count)) }}</div>
              </td>
              <td class="cell-size">
                <div class="numeric">{{ formatBytes(item.size_bytes) }}</div>
                <div class="tiny faint">{{ formatRelTime(item.last_updated_at) }}</div>
              </td>
              <td class="nowrap"><SafetyBadge :status="item.safety.status" /></td>
              <td class="cell-size tiny faint" style="max-width: 200px">{{ primaryReason(item.safety.reasons)?.text }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="actionbar">
      <div class="actionbar__side">
        <button class="btn" data-nav-back="worktrees" :disabled="selectionCount === 0" @click="goTo('worktrees')">
          {{ messages.overview.adjustSelection }}
        </button>
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
