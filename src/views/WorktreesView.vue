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
import { formatBytes, formatRelTime, displayPath } from "../format";
import { messages } from "../i18n";

type FilterKey = "all" | SafetyStatus;

const filter = ref<FilterKey>("all");
const query = ref("");
const expanded = ref<string[]>([]);

const root = computed(() => snapshot.value?.root ?? "");

const filtered = computed<WorktreeRecord[]>(() => {
  const q = query.value.trim().toLowerCase();
  return worktrees.value.filter((item) => {
    if (filter.value !== "all" && item.safety.status !== filter.value) return false;
    if (!q) return true;
    return item.path.toLowerCase().includes(q) || (item.branch || "").toLowerCase().includes(q);
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
  <div class="stack">
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
        <table class="table">
          <thead>
            <tr>
              <th class="cell-check">
                <input v-model="allSelected" class="check" type="checkbox" :disabled="selectable.length === 0" />
              </th>
              <th>{{ messages.worktrees.columns.branch }}</th>
              <th>{{ messages.worktrees.columns.status }}</th>
              <th>{{ messages.worktrees.columns.agent }}</th>
              <th>{{ messages.worktrees.columns.lastActivity }}</th>
              <th class="cell-size">{{ messages.worktrees.columns.size }}</th>
              <th>{{ messages.worktrees.columns.safety }}</th>
              <th style="width: 68px"></th>
            </tr>
          </thead>
          <tbody>
            <template v-for="item in filtered" :key="item.path">
              <tr>
                <td class="cell-check">
                  <input
                    class="check"
                    type="checkbox"
                    :checked="isSelectedWorktree(item.path)"
                    :disabled="item.safety.status === 'Unsafe'"
                    :title="item.safety.status === 'Unsafe' ? messages.common.unsafeTooltip : ''"
                    @change="toggleWorktree(item.path)"
                  />
                </td>
                <td>
                  <div>{{ item.branch || messages.common.detachedHead }}</div>
                  <div class="mono tiny faint">{{ displayPath(item.path, root) }}</div>
                </td>
                <td class="nowrap tiny">{{ stateText(item) }}</td>
                <td class="nowrap"><span class="chip">{{ item.agent || messages.common.emDash }}</span></td>
                <td class="nowrap tiny">{{ formatRelTime(item.last_updated_at) }}</td>
                <td class="cell-size numeric">{{ formatBytes(item.size_bytes) }}</td>
                <td class="nowrap"><SafetyBadge :status="item.safety.status" /></td>
                <td class="nowrap">
                  <button class="btn btn--ghost" @click="toggleExpanded(item.path)">
                    {{ isExpanded(item.path) ? messages.worktrees.collapse : messages.worktrees.details }}
                  </button>
                </td>
              </tr>
              <tr v-if="isExpanded(item.path)">
                <td colspan="8" style="padding: 0">
                  <div class="detail">
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
                      <div class="stack stack--tight">
                        <h3>{{ messages.worktrees.reasons }}</h3>
                        <ReasonList :reasons="item.safety.reasons" />
                        <div v-if="item.untracked_file_list.length > 0">
                          <div class="field__label">{{ messages.worktrees.untrackedLabel }}</div>
                          <div class="untracked-list">
                            <div v-for="file in item.untracked_file_list" :key="file">{{ file }}</div>
                          </div>
                        </div>
                      </div>
                    </div>
                  </div>
                </td>
              </tr>
            </template>
            <tr v-if="filtered.length === 0">
              <td colspan="8"><div class="empty">{{ messages.worktrees.empty }}</div></td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="actionbar">
      <span class="muted">
        {{ " " + messages.worktrees.selectedSummary(selectedSummary.worktrees, selectedSummary.artifacts) }} <strong>{{ formatBytes(selectedSummary.bytes) }}</strong>
      </span>
      <span style="flex: 1"></span>
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
</template>
