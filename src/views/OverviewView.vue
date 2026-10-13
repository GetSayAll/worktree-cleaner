<script setup lang="ts">
import { computed } from "vue";
import { artifacts, counts, goTo, primaryReason, selectedSummary, snapshot, worktrees } from "../state";
import SafetyBadge from "../components/SafetyBadge.vue";
import StatCard from "../components/StatCard.vue";
import { formatBytes, formatCount, formatRelTime, displayPath } from "../format";

const root = computed(() => snapshot.value?.root ?? "");

const topWorktrees = computed(() =>
  [...worktrees.value].sort((a, b) => b.size_bytes - a.size_bytes).slice(0, 5)
);

const topArtifacts = computed(() =>
  [...artifacts.value].sort((a, b) => (b.size_bytes ?? 0) - (a.size_bytes ?? 0)).slice(0, 5)
);

const safeBytes = computed(() =>
  worktrees.value.filter((w) => w.safety.status === "Safe").reduce((sum, w) => sum + w.size_bytes, 0) +
  artifacts.value.filter((a) => a.safety.status === "Safe").reduce((sum, a) => sum + (a.size_bytes ?? 0), 0)
);

const reviewCount = computed(() => counts.value.worktree.Review + counts.value.artifact.Review);
const unsafeCount = computed(() => counts.value.worktree.Unsafe + counts.value.artifact.Unsafe);
const selectionCount = computed(() => selectedSummary.value.worktrees + selectedSummary.value.artifacts);
</script>

<template>
  <div class="stack">
    <div class="spread">
      <div>
        <h1>概览</h1>
        <p class="faint tiny mono">{{ root }} · 默认分支 {{ snapshot?.default_branch || "未知" }}</p>
      </div>
      <div class="row-wrap">
        <button class="btn" @click="goTo('worktrees')">查看 Worktree</button>
        <button class="btn" @click="goTo('artifacts')">查看构建产物</button>
      </div>
    </div>

    <div class="stat-grid stat-grid--4">
      <StatCard
        label="可安全删除"
        tone="safe"
        value-tone
        :value="String(counts.worktree.Safe + counts.artifact.Safe)"
        :sub="`合计 ${formatBytes(safeBytes)} · Safe 默认已勾选`"
      />
      <StatCard
        label="需要确认"
        tone="review"
        value-tone
        :value="String(reviewCount)"
        sub="未合并 / 近期有改动 / 无法完成检查"
      />
      <StatCard
        label="不可删除"
        tone="unsafe"
        value-tone
        :value="String(unsafeCount)"
        sub="主工作区 / 有改动 / 被锁定 / 已提交进仓库"
      />
      <StatCard
        label="预计可释放"
        tone="accent"
        :value="formatBytes(selectedSummary.bytes)"
        :sub="`已选 ${selectionCount} 项 · Worktree ${selectedSummary.worktrees} · 产物 ${selectedSummary.artifacts}`"
      />
    </div>

    <div v-for="warning in snapshot?.warnings || []" :key="warning" class="banner banner--warn">
      <span>⚠</span>
      <span>
        <span class="banner__title">扫描未完整完成 · </span>{{ warning }}
      </span>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>体积最大的 Worktree</h2>
        <button class="btn btn--ghost" @click="goTo('worktrees')">查看全部 {{ worktrees.length }} 个</button>
      </div>
      <div class="card__body card__body--flush">
        <table class="table">
          <tbody>
            <tr v-for="item in topWorktrees" :key="item.path">
              <td>
                <div class="mono">{{ displayPath(item.path, root) }}</div>
                <div class="tiny faint">{{ item.branch || "（detached）" }}{{ item.is_main ? " · 主工作区" : "" }}</div>
              </td>
              <td class="nowrap">
                <span class="chip">{{ item.agent || "未知 Agent" }}</span>
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
        <h2>体积最大的构建产物</h2>
        <button class="btn btn--ghost" @click="goTo('artifacts')">查看全部 {{ artifacts.length }} 个</button>
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
                <div class="tiny faint">{{ item.ecosystem }} · {{ formatCount(item.file_count) }} 个文件</div>
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
      <span :class="selectionCount > 0 ? 'muted' : 'faint'">
        已选 {{ selectionCount }} 项 · 预计释放 <strong>{{ formatBytes(selectedSummary.bytes) }}</strong>
      </span>
      <span v-if="selectionCount === 0" class="faint tiny">先在 Worktree 或构建产物页勾选，Safe 项已默认选中</span>
      <span style="flex: 1"></span>
      <button class="btn" :disabled="selectionCount === 0" @click="goTo('worktrees')">调整选择</button>
      <button class="btn btn--primary" :disabled="selectionCount === 0" @click="goTo('plan')">生成清理计划</button>
    </div>
  </div>
</template>
