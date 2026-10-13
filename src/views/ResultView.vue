<script setup lang="ts">
import { computed, ref } from "vue";
import { goTo, lastResult, projectPath, resultSummary, snapshot, startScan } from "../state";
import StatCard from "../components/StatCard.vue";
import { formatBytes, displayPath } from "../format";
import { messages } from "../i18n";

const root = computed(() => snapshot.value?.root ?? "");
const trashNotice = ref(false);

const ACTION_LABEL: Record<string, string> = {
  "worktree-remove": messages.common.action.worktreeRemove,
  "trash-move": messages.common.action.trashMove,
  prune: messages.common.action.prune
};

const OUTCOME_LABEL: Record<string, string> = {
  ok: messages.common.outcome.ok,
  skipped: messages.common.outcome.skipped,
  failed: messages.common.outcome.failed
};

const okCount = computed(() => lastResult.value.filter((item) => item.result === "ok").length);

// The prototype has no system integration, so this only surfaces a notice. It must
// not reset the session: an earlier version called resetToWelcome and wiped the
// results the user is looking at.
function openTrash() {
  trashNotice.value = true;
  window.setTimeout(() => (trashNotice.value = false), 2400);
}
</script>

<template>
  <div class="stack">
    <div class="spread">
      <div>
        <h1>{{ messages.result.title }}</h1>
        <p class="faint tiny mono">{{ projectPath }}</p>
      </div>
      <div class="row-wrap">
        <button class="btn" :title="messages.result.prototypeTitle" @click="openTrash">{{ messages.result.openTrash }}</button>
        <button class="btn" :title="messages.result.prototypeTitle" @click="goTo('history')">{{ messages.result.openLog }}</button>
        <button class="btn btn--primary" @click="startScan(projectPath)">{{ messages.common.rescan }}</button>
        <span v-if="trashNotice" class="faint tiny">{{ messages.result.trashNotice }}</span>
      </div>
    </div>

    <div class="stat-grid stat-grid--4">
      <StatCard
        :label="messages.result.stats.spaceFreed"
        tone="accent"
        :value="formatBytes(resultSummary.freed, { approximate: resultSummary.freed_unknown > 0 })"
        :sub="resultSummary.freed_unknown > 0 ? messages.result.stats.spaceFreedSubUnknown(resultSummary.freed_unknown) : messages.result.stats.spaceFreedSub"
      />
      <StatCard :label="messages.result.stats.succeeded" tone="safe" value-tone :value="String(okCount)" :sub="messages.result.stats.succeededSub" />
      <StatCard :label="messages.result.stats.movedToTrash" :value="String(resultSummary.trashed)" :sub="messages.result.stats.movedToTrashSub" />
      <StatCard
        :label="messages.result.stats.skippedFailed"
        :value="String(resultSummary.failed)"
        :tone="resultSummary.failed > 0 ? 'unsafe' : 'plain'"
        :sub="messages.result.stats.skippedFailedSub"
      />
    </div>

    <div class="card">
      <div class="card__head"><h2>{{ messages.result.perItemTitle }}</h2></div>
      <div class="card__body card__body--flush">
        <table class="table">
          <thead>
            <tr>
              <th>{{ messages.result.columns.action }}</th>
              <th>{{ messages.result.columns.target }}</th>
              <th>{{ messages.result.columns.detail }}</th>
              <th class="cell-outcome">{{ messages.result.columns.result }}</th>
              <th class="cell-size">{{ messages.result.columns.size }}</th>
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
              <td colspan="5"><div class="empty">{{ messages.result.empty }}</div></td>
            </tr>
          </tbody>
        </table>
      </div>
      <div class="card__foot">
        <span class="faint tiny">
          {{ " " + messages.result.footer + " " }}
        </span>
      </div>
    </div>

    <div class="actionbar">
      <span class="muted">{{ messages.result.complete }}</span>
      <span style="flex: 1"></span>
      <button class="btn" @click="goTo('history')">{{ messages.result.viewHistory }}</button>
      <button class="btn btn--primary" @click="goTo('overview')">{{ messages.result.backToOverview }}</button>
    </div>
  </div>
</template>
