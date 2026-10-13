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
  if (item.is_prunable) return "working directory is gone";
  if (!item.has_upstream) return "no upstream branch";
  return `upstream ahead ${item.ahead} / behind ${item.behind}`;
}

function mergeText(item: WorktreeRecord): string {
  if (item.is_main) return "main worktree; not evaluated for merge status";
  if (item.merged_into_default === null) return `cannot be determined whether merged into ${snapshot.value?.default_branch || "default branch"}`;
  return item.merged_into_default
    ? `merged into ${snapshot.value?.default_branch || "default branch"}`
    : `not merged into ${snapshot.value?.default_branch || "default branch"}`;
}

function stateText(item: WorktreeRecord): string {
  if (item.is_prunable) return "directory is gone";
  const parts: string[] = [];
  parts.push(item.dirty_files > 0 ? `dirty ${item.dirty_files}` : "clean");
  if (item.untracked_files > 0) parts.push(`untracked ${item.untracked_files}`);
  if (item.is_locked) parts.push("locked");
  return parts.join(" · ");
}
</script>

<template>
  <div class="stack">
    <div class="spread">
      <h1>Worktree</h1>
      <span class="faint tiny">
        Data source: git worktree list --porcelain · status --porcelain=v2 · merge-base --is-ancestor
      </span>
    </div>

    <div class="row-wrap">
      <div class="segmented">
        <button :class="filter === 'all' ? 'is-active' : ''" @click="filter = 'all'">All {{ worktrees.length }}</button>
        <button :class="filter === 'Safe' ? 'is-active' : ''" @click="filter = 'Safe'">
          Safe to delete {{ statusCount(worktrees, "Safe") }}
        </button>
        <button :class="filter === 'Review' ? 'is-active' : ''" @click="filter = 'Review'">
          Needs review {{ statusCount(worktrees, "Review") }}
        </button>
        <button :class="filter === 'Unsafe' ? 'is-active' : ''" @click="filter = 'Unsafe'">
          Must not delete {{ statusCount(worktrees, "Unsafe") }}
        </button>
      </div>
      <input v-model="query" class="input input--sm" style="max-width: 240px" placeholder="Search by path or branch" />
    </div>

    <div class="card">
      <div class="card__body card__body--flush">
        <table class="table">
          <thead>
            <tr>
              <th class="cell-check">
                <input v-model="allSelected" class="check" type="checkbox" :disabled="selectable.length === 0" />
              </th>
              <th>Branch</th>
              <th>Status</th>
              <th>Agent</th>
              <th>Last activity</th>
              <th class="cell-size">Size</th>
              <th>Safety</th>
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
                    :title="item.safety.status === 'Unsafe' ? 'Must not delete items cannot be selected' : ''"
                    @change="toggleWorktree(item.path)"
                  />
                </td>
                <td>
                  <div>{{ item.branch || "(detached HEAD)" }}</div>
                  <div class="mono tiny faint">{{ displayPath(item.path, root) }}</div>
                </td>
                <td class="nowrap tiny">{{ stateText(item) }}</td>
                <td class="nowrap"><span class="chip">{{ item.agent || "—" }}</span></td>
                <td class="nowrap tiny">{{ formatRelTime(item.last_updated_at) }}</td>
                <td class="cell-size numeric">{{ formatBytes(item.size_bytes) }}</td>
                <td class="nowrap"><SafetyBadge :status="item.safety.status" /></td>
                <td class="nowrap">
                  <button class="btn btn--ghost" @click="toggleExpanded(item.path)">
                    {{ isExpanded(item.path) ? "Collapse" : "Details" }}
                  </button>
                </td>
              </tr>
              <tr v-if="isExpanded(item.path)">
                <td colspan="8" style="padding: 0">
                  <div class="detail">
                    <div class="detail__grid">
                      <div class="stack stack--tight">
                        <h3>Git facts</h3>
                        <dl class="kv">
                          <dt>Full path</dt>
                          <dd class="mono">{{ item.path }}</dd>
                          <dt>HEAD</dt>
                          <dd class="mono">{{ item.head || "—" }}</dd>
                          <dt>Upstream</dt>
                          <dd>{{ upstreamText(item) }}</dd>
                          <dt>Merge status</dt>
                          <dd>{{ mergeText(item) }}</dd>
                          <dt>stash</dt>
                          <dd>{{ item.stash_count }} entries</dd>
                          <dt>Untracked files</dt>
                          <dd>{{ item.untracked_files }}</dd>
                          <dt>Directory size</dt>
                          <dd>{{ formatBytes(item.size_bytes) }}</dd>
                        </dl>
                        <div v-if="item.is_prunable" class="banner banner--info">
                          The working directory no longer exists; only git worktree prune runs to clean up metadata in .git/worktrees. No files are touched.
                        </div>
                      </div>
                      <div class="stack stack--tight">
                        <h3>Reasons</h3>
                        <ReasonList :reasons="item.safety.reasons" />
                        <div v-if="item.untracked_file_list.length > 0">
                          <div class="field__label">Untracked files (removing this worktree requires --force)</div>
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
              <td colspan="8"><div class="empty">No worktrees match the current filters</div></td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="actionbar">
      <span class="muted">
        Selected {{ selectedSummary.worktrees }} worktrees · {{ selectedSummary.artifacts }} artifacts · Estimated reclaimable
        <strong>{{ formatBytes(selectedSummary.bytes) }}</strong>
      </span>
      <span style="flex: 1"></span>
      <button class="btn" @click="goTo('artifacts')">Continue to artifacts</button>
      <button
        class="btn btn--primary"
        :disabled="selectedSummary.worktrees + selectedSummary.artifacts === 0"
        @click="goTo('plan')"
      >
        Generate cleanup plan
      </button>
    </div>
  </div>
</template>
