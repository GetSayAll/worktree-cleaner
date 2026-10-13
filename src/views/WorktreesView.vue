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
import { compareByActionability, compareSizeNullsFirst, formatBytes, formatRelTime, displayPath, isUnder } from "../format";
import { messages } from "../i18n";

type FilterKey = "all" | SafetyStatus;

const filter = ref<FilterKey>("all");
const query = ref("");
const expanded = ref<string[]>([]);

const root = computed(() => snapshot.value?.root ?? "");

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
 * What the list is ordered by, and why.
 *
 * Safe first, then the ones that need a decision, then the ones that cannot be
 * touched — and largest first inside each level. The page used to keep the scan's
 * own order, which is whatever Git listed: the removable worktrees were scattered
 * among the main worktree and the dirty ones, and finding the ones worth acting on
 * meant reading every row. Now the order answers "where do I start" without the
 * reader sorting anything.
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
  return [...matched].sort((a, b) => {
    const byStatus = compareByActionability(a, b);
    if (byStatus !== 0) return byStatus;
    return compareSizeNullsFirst(a.size_bytes, b.size_bytes);
  });
});

const selectable = computed(() => filtered.value.filter((item) => item.safety.status !== "Unsafe").map((item) => item.path));

const allSelected = computed({
  get: () => selectable.value.length > 0 && selectable.value.every((p) => isSelectedWorktree(p)),
  set: (value: boolean) => toggleGroupWorktrees(selectable.value, value)
});

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
  if (item.merged_into_default === null) return messages.worktrees.mergeUnknown(snapshot.value?.default_branch || messages.common.defaultBranch);
  return item.merged_into_default
    ? messages.worktrees.mergeYes(snapshot.value?.default_branch || messages.common.defaultBranch)
    : messages.worktrees.mergeNo(snapshot.value?.default_branch || messages.common.defaultBranch);
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
    <div class="spread">
      <h1>{{ messages.worktrees.title }}</h1>
      <span class="faint tiny">{{ " " + messages.worktrees.dataSource + " " }}</span>
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

    <div class="card">
      <div class="card__body card__body--flush">
        <div class="select-all">
          <input v-model="allSelected" class="check" type="checkbox" :disabled="selectable.length === 0" />
          <span>{{ messages.worktrees.selectAll }}</span>
        </div>
        <template v-for="item in filtered" :key="item.path">
          <div class="item-row">
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
                <span class="chip">{{ item.agent || messages.common.emDash }}</span>
              </div>
              <div class="item-row__meta">
                <span class="item-row__where" :title="item.path">{{ displayPath(item.path, root) }}</span>
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
        <div v-if="filtered.length === 0" class="empty">{{ messages.worktrees.empty }}</div>
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
