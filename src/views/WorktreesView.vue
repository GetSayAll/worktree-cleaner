<script setup lang="ts">
import { computed, ref } from "vue";
import type { SafetyStatus, WorktreeRecord } from "../types";
import {
  goTo,
  isSelectedWorktree,
  selectedSummary,
  snapshot,
  statusCount,
  toggleGroupWorktrees,
  toggleWorktree,
  worktrees
} from "../state";
import SafetyBadge from "../components/SafetyBadge.vue";
import ReasonList from "../components/ReasonList.vue";
import RevealPathButton from "../components/RevealPathButton.vue";
import { basename, compareSizeNullsLast, formatBytes, formatRelTime, displayPath, isUnder } from "../format";
import { messages } from "../i18n";
import { reviewLane, type ReviewLane } from "../worktreeGroups";

type FilterKey = "all" | SafetyStatus;

interface ReviewGroup {
  key: ReviewLane;
  items: WorktreeRecord[];
  /** Attention rows must be selected one by one. */
  selectable: boolean;
}

interface Section {
  key: string;
  group: ReviewGroup | null;
  items: WorktreeRecord[];
}

const filter = ref<FilterKey>("all");
const query = ref("");
const expanded = ref<string[]>([]);

const root = computed(() => snapshot.value?.root ?? "");

/** The remote-tracking ref the merge check actually uses, for the protected hint. */
const defaultBranchRef = computed(() => {
  const branch = snapshot.value?.default_branch;
  if (!branch) return "";
  return branch.startsWith("origin/") ? branch : `origin/${branch}`;
});

/**
 * How many disk-hygiene rules the core reported as violated for this repository.
 *
 * Counted from `hygiene.rules` — the same list the hygiene page and the nav badge
 * count — so the three never disagree. The verdict itself is the core's: this only
 * decides whether to show the banner (D9).
 *
 * It belongs on this page because this is where a cleanup starts. A repository whose
 * worktree count is over the cap, or that leaves merged worktrees behind, refills
 * after every pass; without saying so, the user cleans the same repository again next
 * month and concludes the tool did not work.
 */
const hygieneViolations = computed(
  () => snapshot.value?.hygiene.rules.filter((rule) => rule.level === "violation").length ?? 0
);

/**
 * Whether a worktree lives outside the directory the user picked.
 *
 * Telling, not deciding: `git worktree add` accepts any path, so a worktree of the
 * chosen repository can be a sibling directory, a path under `~/worktrees`, or a
 * scratch directory in `/tmp`. Without this the page shows a path the reader cannot
 * place and does not explain why it is there.
 */
function isOutsideRoot(path: string): boolean {
  return root.value.length > 0 && !isUnder(path, root.value);
}

/**
 * The worktree list is ordered by measured size, largest first. An unmeasurable
 * size stays last because "unknown" is not a number; equal sizes fall back to
 * the path so the order is stable between scans.
 */
const filtered = computed<WorktreeRecord[]>(() => {
  const q = query.value.trim().toLowerCase();
  const matched = worktrees.value.filter((item) => {
    if (filter.value !== "all" && item.safety.status !== filter.value) return false;
    if (!q) return true;
    return item.path.toLowerCase().includes(q) || (item.branch || "").toLowerCase().includes(q);
  });
  // Copied before sorting: `worktrees` is a computed over the snapshot, and
  // sorting in place would reorder the array the other pages read.
  return [...matched].sort(
    (a, b) =>
      compareSizeNullsLast(a.size_bytes, b.size_bytes) ||
      basename(a.path).localeCompare(basename(b.path))
  );
});

/**
 * Review rows grouped by what removing them can lose.
 *
 * The default selection still contains only `Safe` rows. These groups change
 * how a reader confirms `Review`, not the verdict: each group action is an
 * explicit selection, and the executor re-checks every item afterwards.
 */
const reviewGroups = computed<ReviewGroup[]>(() => {
  const buckets: Record<ReviewLane, WorktreeRecord[]> = {
    batch: [],
    untracked: [],
    attention: []
  };
  for (const item of filtered.value) {
    const lane = reviewLane(item);
    if (lane) buckets[lane].push(item);
  }
  return (["batch", "untracked", "attention"] as const)
    .map((key) => ({ key, items: buckets[key], selectable: key !== "attention" }))
    .filter((group) => group.items.length > 0);
});

const reviewCount = computed(
  () => worktrees.value.filter((item) => item.safety.status === "Review").length
);

/**
 * One section per Review lane; every other filter stays one flat section. The
 * row markup is shared, so a group header never duplicates or replaces a row.
 */
const sections = computed<Section[]>(() => {
  if (filter.value === "Review") {
    return reviewGroups.value.map((group) => ({
      key: group.key,
      group,
      items: group.items
    }));
  }
  return filtered.value.length > 0
    ? [{ key: "flat", group: null, items: filtered.value }]
    : [];
});

const selectable = computed(() =>
  filter.value === "Review"
    ? []
    : filtered.value.filter((item) => item.safety.status !== "Unsafe").map((item) => item.path)
);

const allSelected = computed({
  get: () => selectable.value.length > 0 && selectable.value.every((p) => isSelectedWorktree(p)),
  set: (value: boolean) => toggleGroupWorktrees(selectable.value, value)
});

function reviewGroupAllSelected(group: ReviewGroup | null): boolean {
  return Boolean(group && group.items.length > 0 && group.items.every((item) => isSelectedWorktree(item.path)));
}

function toggleReviewGroup(group: ReviewGroup | null) {
  if (!group || !group.selectable) return;
  toggleGroupWorktrees(
    group.items.map((item) => item.path),
    !reviewGroupAllSelected(group)
  );
}

function reviewGroupSize(group: ReviewGroup | null): string {
  if (!group) return "";
  const bytes = group.items.reduce((sum, item) => sum + (item.size_bytes ?? 0), 0);
  const unknown = group.items.filter((item) => item.size_bytes === null).length;
  return formatBytes(bytes, { approximate: unknown > 0 });
}

function reviewGroupTitle(key: ReviewLane): string {
  switch (key) {
    case "batch":
      return messages.worktrees.reviewGroupBatchTitle;
    case "untracked":
      return messages.worktrees.reviewGroupUntrackedTitle;
    case "attention":
      return messages.worktrees.reviewGroupAttentionTitle;
  }
}

function reviewGroupImpact(key: ReviewLane): string {
  switch (key) {
    case "batch":
      return messages.worktrees.reviewGroupBatchImpact;
    case "untracked":
      return messages.worktrees.reviewGroupUntrackedImpact;
    case "attention":
      return messages.worktrees.reviewGroupAttentionImpact;
  }
}

function reviewGroupActionLabel(group: ReviewGroup | null): string {
  if (!group) return "";
  if (reviewGroupAllSelected(group)) return messages.worktrees.reviewGroupClear;
  return messages.worktrees.reviewGroupSelect(group.items.length);
}

function showReviewGroups() {
  filter.value = "Review";
}

function toggleExpanded(path: string) {
  expanded.value = expanded.value.includes(path)
    ? expanded.value.filter((p) => p !== path)
    : [...expanded.value, path];
}

function isExpanded(path: string): boolean {
  return expanded.value.includes(path);
}

function upstreamText(item: WorktreeRecord): string {
  if (item.is_prunable) return messages.worktrees.upstreamGone;
  if (!item.has_upstream) return messages.worktrees.upstreamNone;
  return messages.worktrees.upstreamCounts(item.ahead, item.behind);
}

function mergeText(item: WorktreeRecord): string {
  if (item.is_main) return messages.worktrees.mergeMain;
  // The merge check uses the remote-tracking default branch (`origin/main`), not
  // whatever local branch the main workspace has checked out. Show the ref the
  // command actually used, so the detail panel agrees with the evidence line.
  const branch = defaultBranchRef.value || messages.common.defaultBranch;
  // Absorption is checked before the ancestry answer: the commit is not an ancestor,
  // so `merged_into_default` is false here, and reporting that alone would hide the
  // one fact that matters — the change is already in the default branch.
  if (item.absorbed_into_default) return messages.worktrees.mergeAbsorbed(branch);
  if (item.merged_into_default === null) return messages.worktrees.mergeUnknown(branch);
  return item.merged_into_default
    ? messages.worktrees.mergeYes(branch)
    : messages.worktrees.mergeNo(branch);
}

function stateText(item: WorktreeRecord): string {
  if (item.is_prunable) return messages.worktrees.stateGone;
  const parts: string[] = [];
  parts.push(item.dirty_files > 0 ? messages.worktrees.stateDirty(item.dirty_files) : messages.worktrees.stateClean);
  if (item.untracked_files > 0) parts.push(messages.worktrees.stateUntracked(item.untracked_files));
  if (item.is_locked) parts.push(messages.worktrees.stateLocked);
  return parts.join(" · ");
}
</script>

<template>
  <div class="stack page--wide">
    <h1>{{ messages.worktrees.title }}</h1>

    <p class="rules-summary"><strong>{{ messages.rules.shortLabel }}</strong>{{ messages.rules.worktreesSummary }}</p>

    <div v-if="hygieneViolations > 0" class="banner banner--warn">
      <span>!</span>
      <span>
        <span class="banner__title">{{ messages.worktrees.hygieneTitle }}</span>
        {{ messages.worktrees.hygieneBody(hygieneViolations) }}
      </span>
      <span style="flex: 1"></span>
      <button class="btn btn--ghost" @click="goTo('hygiene')">{{ messages.worktrees.hygieneAction }}</button>
    </div>

    <div class="row-wrap">
      <div class="segmented">
        <button :class="filter === 'all' ? 'is-active' : ''" @click="filter = 'all'">{{ messages.worktrees.filterAll(worktrees.length) }}</button>
        <button :class="filter === 'Safe' ? 'is-active' : ''" @click="filter = 'Safe'">
          {{ " " + messages.worktrees.filterSafe(statusCount(worktrees, "Safe")) }}
        </button>
        <button :class="filter === 'Review' ? 'is-active' : ''" @click="filter = 'Review'">
          {{ " " + messages.worktrees.filterReview(statusCount(worktrees, "Review")) }}
        </button>
        <button :class="filter === 'Unsafe' ? 'is-active' : ''" @click="filter = 'Unsafe'">
          {{ " " + messages.worktrees.filterUnsafe(statusCount(worktrees, "Unsafe")) }}
        </button>
      </div>
      <input v-model="query" class="input input--sm" style="max-width: 240px" :placeholder="messages.worktrees.searchPlaceholder" />
    </div>

    <div v-if="filter === 'all' && reviewCount > 0" class="review-hint">
      <span>{{ messages.worktrees.reviewGroupHint(reviewCount) }}</span>
      <span style="flex: 1"></span>
      <button class="btn btn--ghost" @click="showReviewGroups">{{ messages.worktrees.reviewGroupHintAction }}</button>
    </div>

    <div class="card">
      <div class="card__body card__body--flush">
        <div v-if="filter !== 'Review'" class="select-all">
          <input v-model="allSelected" class="check" type="checkbox" :disabled="selectable.length === 0" />
          <span>{{ messages.worktrees.selectAll }}</span>
        </div>
        <template v-for="section in sections" :key="section.key">
          <div v-if="section.group" class="review-group-head" :data-review-group="section.group.key">
            <div class="review-group-head__body">
              <div class="row-wrap">
                <span class="review-group-head__title">{{ reviewGroupTitle(section.group.key) }}</span>
                <span class="chip">{{ messages.worktrees.reviewGroupCount(section.items.length) }}</span>
                <span class="faint tiny">{{ reviewGroupSize(section.group) }}</span>
              </div>
              <p class="review-group-head__impact">{{ reviewGroupImpact(section.group.key) }}</p>
            </div>
            <div class="review-group-head__action">
              <button
                v-if="section.group.selectable"
                class="btn"
                :aria-pressed="reviewGroupAllSelected(section.group)"
                @click="toggleReviewGroup(section.group)"
              >
                {{ reviewGroupActionLabel(section.group) }}
              </button>
              <span v-else class="faint tiny">{{ messages.worktrees.reviewGroupIndividual }}</span>
            </div>
          </div>

          <template v-for="item in section.items" :key="item.path">
          <div class="item-row" :class="{ 'item-row--main': item.is_main }">
            <div class="item-row__check">
              <input
                class="check"
                type="checkbox"
                :checked="isSelectedWorktree(item.path)"
                :disabled="item.safety.status === 'Unsafe'"
                :title="item.safety.status === 'Unsafe' ? messages.common.unsafeTooltip : ''"
                @change="toggleWorktree(item.path)"
              />
            </div>
            <div class="item-row__main">
              <div class="item-row__head">
                <span class="item-row__name">{{ item.branch || messages.common.detachedHead }}</span>
                <span v-if="item.is_main" class="chip chip--protected" :title="messages.worktrees.mainWorkspaceHint(defaultBranchRef)">{{ messages.worktrees.mainWorkspaceBadge }}</span>
                <span v-if="item.agent" class="chip">{{ item.agent }}</span>
                <span v-else-if="!item.is_main" class="chip" :title="messages.common.unknownAgentHint">{{ messages.common.unknownAgent }}</span>
              </div>
              <div class="item-row__meta">
                <span class="item-row__where" :title="item.path">{{ item.is_main ? basename(item.path) : displayPath(item.path, root) }}</span>
                <span
                  v-if="isOutsideRoot(item.path)"
                  class="chip chip--outside"
                  :title="messages.worktrees.outsideRootHint"
                >{{ messages.worktrees.outsideRoot }}</span>
                <span>· {{ stateText(item) }}</span>
                <span>· {{ formatRelTime(item.last_updated_at) }}</span>
              </div>
              <div class="item-row__foot">
                <div class="item-row__reasons">
                  <ReasonList :reasons="item.safety.reasons" />
                </div>
                <button class="btn btn--ghost" @click="toggleExpanded(item.path)">
                  {{ isExpanded(item.path) ? messages.worktrees.collapse : messages.worktrees.details }}
                </button>
                <RevealPathButton
                  v-if="item.safety.status !== 'Safe' || !item.agent"
                  :path="item.path"
                />
              </div>
            </div>
            <div class="item-row__figures">
              <span class="item-row__size">{{ formatBytes(item.size_bytes) }}</span>
              <SafetyBadge :status="item.safety.status" />
            </div>
          </div>

          <div v-if="isExpanded(item.path)" class="detail">
            <div class="detail__grid">
              <div class="stack stack--tight">
                <h3>{{ messages.worktrees.gitFacts }}</h3>
                <dl class="kv">
                  <dt>{{ messages.worktrees.facts.fullPath }}</dt>
                  <dd class="mono">{{ item.path }}</dd>
                  <dt>{{ messages.worktrees.facts.head }}</dt>
                  <dd class="mono">{{ item.head || messages.common.emDash }}</dd>
                  <dt>{{ messages.worktrees.facts.upstream }}</dt>
                  <dd>{{ upstreamText(item) }}</dd>
                  <dt>{{ messages.worktrees.facts.mergeStatus }}</dt>
                  <dd>{{ mergeText(item) }}</dd>
                  <dt>{{ messages.worktrees.facts.stash }}</dt>
                  <dd>{{ messages.worktrees.stashEntries(item.stash_count) }}</dd>
                  <dt>{{ messages.worktrees.facts.untrackedFiles }}</dt>
                  <dd>{{ item.untracked_files }}</dd>
                  <dt>{{ messages.worktrees.facts.directorySize }}</dt>
                  <dd>{{ formatBytes(item.size_bytes) }}</dd>
                </dl>
                <div v-if="item.is_prunable" class="banner banner--info">
                  {{ " " + messages.worktrees.prunableBanner + " " }}
                </div>
              </div>
              <!-- The reasons are on the row itself now, so this panel is what the
                   row cannot show: the Git facts behind the verdict, and the
                   untracked files that make a removal a force. -->
              <div class="stack stack--tight">
                <div v-if="item.untracked_file_list.length > 0">
                  <div class="field__label">{{ messages.worktrees.untrackedLabel }}</div>
                  <div class="untracked-list">
                    <div v-for="file in item.untracked_file_list" :key="file">{{ file }}</div>
                  </div>
                </div>
              </div>
            </div>
          </div>
          </template>
        </template>
        <div v-if="sections.length === 0" class="empty">{{ messages.worktrees.empty }}</div>
      </div>
    </div>

    <!-- Two ways forward, because this step is one of two: pick worktrees, then
         pick artifacts. Both sit on the right so "back" stays on the left on every
         page, and the shorter hop ("look at the artifacts") is the quiet one. -->
    <div class="actionbar">
      <div class="actionbar__side">
        <button class="btn" data-nav-back="overview" @click="goTo('overview')">
          {{ messages.nav.overview }}
        </button>
      </div>
      <span class="actionbar__summary muted">
        {{ " " + messages.worktrees.selectedSummary(selectedSummary.worktrees, selectedSummary.artifacts) }} <strong>{{ formatBytes(selectedSummary.bytes, { approximate: selectedSummary.unknown > 0 }) }}</strong>
      </span>
      <div class="actionbar__side" style="margin-left: auto">
        <button class="btn" @click="goTo('artifacts')">{{ messages.worktrees.continueToArtifacts }}</button>
        <button
          class="btn btn--primary"
          :disabled="selectedSummary.worktrees + selectedSummary.artifacts === 0"
          @click="goTo('plan')"
        >
          {{ " " + messages.worktrees.generatePlan + " " }}
        </button>
      </div>
    </div>
  </div>
</template>
