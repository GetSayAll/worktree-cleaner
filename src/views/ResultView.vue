<script setup lang="ts">
import { computed } from "vue";
import { goTo, lastResult, projectPath, resetToWelcome, resultSummary, snapshot, startScan } from "../state";
import StatCard from "../components/StatCard.vue";
import { formatBytes, displayPath } from "../format";

const root = computed(() => snapshot.value?.root ?? "");

const ACTION_LABEL: Record<string, string> = {
  "worktree-remove": "移除 Worktree",
  "trash-move": "移入废纸篓",
  prune: "清理元数据"
};

const OUTCOME_LABEL: Record<string, string> = {
  ok: "成功",
  skipped: "已跳过",
  failed: "失败"
};

const okCount = computed(() => lastResult.value.filter((item) => item.result === "ok").length);
</script>

<template>
  <div class="stack">
    <div class="spread">
      <div>
        <h1>执行结果</h1>
        <p class="faint tiny mono">{{ projectPath }}</p>
      </div>
      <div class="row-wrap">
        <button class="btn" title="原型未接入系统调用" @click="resetToWelcome">打开废纸篓</button>
        <button class="btn" title="原型未接入系统调用" @click="goTo('history')">打开日志</button>
        <button class="btn btn--primary" @click="startScan(projectPath)">重新扫描</button>
      </div>
    </div>

    <div class="stat-grid stat-grid--4">
      <StatCard label="释放空间" tone="accent" :value="formatBytes(resultSummary.freed)" sub="已计入废纸篓与 Worktree" />
      <StatCard label="成功条目" tone="safe" value-tone :value="String(okCount)" sub="含元数据清理" />
      <StatCard label="移入废纸篓" :value="String(resultSummary.trashed)" sub="可在废纸篓中找回" />
      <StatCard
        label="跳过 / 失败"
        :value="String(resultSummary.failed)"
        :tone="resultSummary.failed > 0 ? 'unsafe' : 'plain'"
        sub="原因见下表"
      />
    </div>

    <div class="card">
      <div class="card__head"><h2>逐条结果</h2></div>
      <div class="card__body card__body--flush">
        <table class="table">
          <thead>
            <tr>
              <th>动作</th>
              <th>目标</th>
              <th>说明</th>
              <th class="cell-outcome">结果</th>
              <th class="cell-size">体积</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(item, index) in lastResult" :key="`${item.target}-${index}`">
              <td class="nowrap tiny">{{ ACTION_LABEL[item.action] || item.action }}</td>
              <td class="mono">{{ displayPath(item.target, root) }}</td>
              <td class="tiny faint">{{ item.detail }}</td>
              <td class="cell-outcome">
                <span class="outcome" :class="`outcome--${item.result}`">{{ OUTCOME_LABEL[item.result] }}</span>
              </td>
              <td class="cell-size numeric">{{ formatBytes(item.size_bytes) }}</td>
            </tr>
            <tr v-if="lastResult.length === 0">
              <td colspan="5"><div class="empty">没有执行记录</div></td>
            </tr>
          </tbody>
        </table>
      </div>
      <div class="card__foot">
        <span class="faint tiny">
          失败条目不影响其它条目；已写入本机操作日志。删除动作不永久生效，目录类目标均可从废纸篓恢复。
        </span>
      </div>
    </div>

    <div class="actionbar">
      <span class="muted">清理完成，可重新扫描确认结果</span>
      <span style="flex: 1"></span>
      <button class="btn" @click="goTo('history')">查看历史</button>
      <button class="btn btn--primary" @click="goTo('overview')">返回概览</button>
    </div>
  </div>
</template>
