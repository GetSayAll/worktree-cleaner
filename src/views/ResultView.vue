<script setup lang="ts">
import { computed } from "vue";
import { goTo, lastResult, projectPath, resetToWelcome, resultSummary, snapshot, startScan } from "../state";
import StatCard from "../components/StatCard.vue";
import { formatBytes, displayPath } from "../format";

const root = computed(() => snapshot.value?.root ?? "");

const ACTION_LABEL: Record<string, string> = {
  "worktree-remove": "Remove Worktree",
  "trash-move": "Move to Trash",
  prune: "Clean up metadata"
};

const OUTCOME_LABEL: Record<string, string> = {
  ok: "Succeeded",
  skipped: "Skipped",
  failed: "Failed"
};

const okCount = computed(() => lastResult.value.filter((item) => item.result === "ok").length);
</script>

<template>
  <div class="stack">
    <div class="spread">
      <div>
        <h1>Result</h1>
        <p class="faint tiny mono">{{ projectPath }}</p>
      </div>
      <div class="row-wrap">
        <button class="btn" title="Prototype does not call into the system" @click="resetToWelcome">Open Trash</button>
        <button class="btn" title="Prototype does not call into the system" @click="goTo('history')">Open log</button>
        <button class="btn btn--primary" @click="startScan(projectPath)">Rescan</button>
      </div>
    </div>

    <div class="stat-grid stat-grid--4">
      <StatCard label="Space freed" tone="accent" :value="formatBytes(resultSummary.freed)" sub="includes Trash moves and worktrees" />
      <StatCard label="Succeeded" tone="safe" value-tone :value="String(okCount)" sub="includes metadata cleanup" />
      <StatCard label="Moved to Trash" :value="String(resultSummary.trashed)" sub="recoverable from Trash" />
      <StatCard
        label="Skipped / failed"
        :value="String(resultSummary.failed)"
        :tone="resultSummary.failed > 0 ? 'unsafe' : 'plain'"
        sub="see the table below for reasons"
      />
    </div>

    <div class="card">
      <div class="card__head"><h2>Per-item results</h2></div>
      <div class="card__body card__body--flush">
        <table class="table">
          <thead>
            <tr>
              <th>Action</th>
              <th>Target</th>
              <th>Detail</th>
              <th class="cell-outcome">Result</th>
              <th class="cell-size">Size</th>
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
              <td colspan="5"><div class="empty">No execution records</div></td>
            </tr>
          </tbody>
        </table>
      </div>
      <div class="card__foot">
        <span class="faint tiny">
          Failed items do not affect the others; every action is written to the local operation log. Deletions are not permanent, and directory targets can be recovered from Trash.
        </span>
      </div>
    </div>

    <div class="actionbar">
      <span class="muted">Cleanup complete; rescan to confirm the result</span>
      <span style="flex: 1"></span>
      <button class="btn" @click="goTo('history')">View history</button>
      <button class="btn btn--primary" @click="goTo('overview')">Back to overview</button>
    </div>
  </div>
</template>
