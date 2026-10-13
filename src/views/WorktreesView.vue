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
  if (item.is_prunable) return "工作目录已丢失";
  if (!item.has_upstream) return "无上游分支";
  return `上游 ahead ${item.ahead} / behind ${item.behind}`;
}

function mergeText(item: WorktreeRecord): string {
  if (item.is_main) return "主工作区，不参与合并判断";
  if (item.merged_into_default === null) return `无法判定是否合入 ${snapshot.value?.default_branch || "默认分支"}`;
  return item.merged_into_default
    ? `已合入 ${snapshot.value?.default_branch || "默认分支"}`
    : `未合入 ${snapshot.value?.default_branch || "默认分支"}`;
}

function stateText(item: WorktreeRecord): string {
  if (item.is_prunable) return "目录已丢失";
  const parts: string[] = [];
  parts.push(item.dirty_files > 0 ? `脏 ${item.dirty_files}` : "干净");
  if (item.untracked_files > 0) parts.push(`未跟踪 ${item.untracked_files}`);
  if (item.is_locked) parts.push("已锁定");
  return parts.join(" · ");
}
</script>

<template>
  <div class="stack">
    <div class="spread">
      <h1>Worktree</h1>
      <span class="faint tiny">
        数据来源：git worktree list --porcelain · status --porcelain=v2 · merge-base --is-ancestor
      </span>
    </div>

    <div class="row-wrap">
      <div class="segmented">
        <button :class="filter === 'all' ? 'is-active' : ''" @click="filter = 'all'">全部 {{ worktrees.length }}</button>
        <button :class="filter === 'Safe' ? 'is-active' : ''" @click="filter = 'Safe'">
          可安全删除 {{ statusCount(worktrees, "Safe") }}
        </button>
        <button :class="filter === 'Review' ? 'is-active' : ''" @click="filter = 'Review'">
          需要确认 {{ statusCount(worktrees, "Review") }}
        </button>
        <button :class="filter === 'Unsafe' ? 'is-active' : ''" @click="filter = 'Unsafe'">
          不可删除 {{ statusCount(worktrees, "Unsafe") }}
        </button>
      </div>
      <input v-model="query" class="input input--sm" style="max-width: 240px" placeholder="按路径或分支搜索" />
    </div>

    <div class="card">
      <div class="card__body card__body--flush">
        <table class="table">
          <thead>
            <tr>
              <th class="cell-check">
                <input v-model="allSelected" class="check" type="checkbox" :disabled="selectable.length === 0" />
              </th>
              <th>分支</th>
              <th>状态</th>
              <th>Agent</th>
              <th>最后活动</th>
              <th class="cell-size">大小</th>
              <th>安全等级</th>
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
                    :title="item.safety.status === 'Unsafe' ? '不可删除项无法勾选' : ''"
                    @change="toggleWorktree(item.path)"
                  />
                </td>
                <td>
                  <div>{{ item.branch || "（detached HEAD）" }}</div>
                  <div class="mono tiny faint">{{ displayPath(item.path, root) }}</div>
                </td>
                <td class="nowrap tiny">{{ stateText(item) }}</td>
                <td class="nowrap"><span class="chip">{{ item.agent || "—" }}</span></td>
                <td class="nowrap tiny">{{ formatRelTime(item.last_updated_at) }}</td>
                <td class="cell-size numeric">{{ formatBytes(item.size_bytes) }}</td>
                <td class="nowrap"><SafetyBadge :status="item.safety.status" /></td>
                <td class="nowrap">
                  <button class="btn btn--ghost" @click="toggleExpanded(item.path)">
                    {{ isExpanded(item.path) ? "收起" : "详情" }}
                  </button>
                </td>
              </tr>
              <tr v-if="isExpanded(item.path)">
                <td colspan="8" style="padding: 0">
                  <div class="detail">
                    <div class="detail__grid">
                      <div class="stack stack--tight">
                        <h3>Git 事实</h3>
                        <dl class="kv">
                          <dt>完整路径</dt>
                          <dd class="mono">{{ item.path }}</dd>
                          <dt>HEAD</dt>
                          <dd class="mono">{{ item.head || "—" }}</dd>
                          <dt>上游</dt>
                          <dd>{{ upstreamText(item) }}</dd>
                          <dt>合并状态</dt>
                          <dd>{{ mergeText(item) }}</dd>
                          <dt>stash</dt>
                          <dd>{{ item.stash_count }} 条</dd>
                          <dt>未跟踪文件</dt>
                          <dd>{{ item.untracked_files }} 个</dd>
                          <dt>目录体积</dt>
                          <dd>{{ formatBytes(item.size_bytes) }}</dd>
                        </dl>
                        <div v-if="item.is_prunable" class="banner banner--info">
                          工作目录已不存在，只会执行 git worktree prune 清理 .git/worktrees 里的元数据，不动任何文件。
                        </div>
                      </div>
                      <div class="stack stack--tight">
                        <h3>判断原因</h3>
                        <ReasonList :reasons="item.safety.reasons" />
                        <div v-if="item.untracked_file_list.length > 0">
                          <div class="field__label">未跟踪文件（删除该 worktree 时需要 --force）</div>
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
              <td colspan="8"><div class="empty">没有符合条件的 Worktree</div></td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div class="actionbar">
      <span class="muted">
        已选 {{ selectedSummary.worktrees }} 个 Worktree · {{ selectedSummary.artifacts }} 个产物 · 预计释放
        <strong>{{ formatBytes(selectedSummary.bytes) }}</strong>
      </span>
      <span style="flex: 1"></span>
      <button class="btn" @click="goTo('artifacts')">继续选产物</button>
      <button
        class="btn btn--primary"
        :disabled="selectedSummary.worktrees + selectedSummary.artifacts === 0"
        @click="goTo('plan')"
      >
        生成清理计划
      </button>
    </div>
  </div>
</template>
