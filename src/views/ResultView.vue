<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { executeErrorMessage, executing, goTo, lastResult, logError, projectPath, resultSummary, snapshot, startScan } from "../state";
import StatCard from "../components/StatCard.vue";
import RevealPathButton from "../components/RevealPathButton.vue";
import { detailText, formatBytes, displayPath } from "../format";
import { messages } from "../i18n";
import { openTrash, isDesktop } from "../platform";

const desktop = isDesktop();

const root = computed(() => snapshot.value?.root ?? "");

// The result page is the success page, so it has to open at its own top. The
// content pane is shared with the plan page and keeps its scroll position; after
// a run that followed the live rows that position is somewhere in the table,
// which would hide the completion summary.
onMounted(() => {
  const content = document.querySelector(".content");
  if (content) content.scrollTop = 0;
});

// Looked up while rendering: the language can change with this page open.
function actionLabel(action: string): string {
  return (
    {
      "worktree-remove": messages.common.action.worktreeRemove,
      "trash-move": messages.common.action.trashMove,
      prune: messages.common.action.prune
    }[action] ?? action
  );
}

function outcomeLabel(result: string): string {
  return (
    {
      ok: messages.common.outcome.ok,
      skipped: messages.common.outcome.skipped,
      failed: messages.common.outcome.failed
    }[result] ?? result
  );
}

const okCount = computed(() => lastResult.value.filter((item) => item.result === "ok").length);
const skippedTrashMoves = computed(
  () => lastResult.value.filter((item) => item.action === "trash-move" && item.result === "skipped").length
);

const measuredResults = computed(() => lastResult.value.filter((item) => item.size_bytes !== null));
const bySize = (a: { size_bytes: number | null }, b: { size_bytes: number | null }) =>
  (b.size_bytes ?? 0) - (a.size_bytes ?? 0);
const topRemaining = computed(() =>
  measuredResults.value
    .filter((item) => item.result !== "ok")
    .sort(bySize)
    .slice(0, 3)
);
const topHandled = computed(() =>
  measuredResults.value
    .filter((item) => item.result === "ok")
    .sort(bySize)
    .slice(0, 3)
);
const topItems = computed(() => (topRemaining.value.length > 0 ? topRemaining.value : topHandled.value));
const showingRemaining = computed(() => topRemaining.value.length > 0);

/**
 * The failure of a system action this page offers.
 *
 * Opening the Trash or the log can fail — the path may not exist, the opener may
 * refuse. None of those should leave a click with no effect and no explanation.
 */
const trashOpenFailed = ref(false);

async function openSystemTrash() {
  trashOpenFailed.value = false;
  try {
    await openTrash();
  } catch {
    trashOpenFailed.value = true;
  }
}

/**
 * Splits the core's `log_error` into the path it could not write and what went
 * wrong.
 *
 * The core formats it as `"<path> (<error>)"`, and the two halves need different
 * words around them: the path says where, the error says why.
 */
const logFailure = computed(() => {
  const raw = logError.value;
  if (!raw) return null;
  const cut = raw.indexOf(" (");
  if (cut === -1) return { path: raw, error: "" };
  const error = raw.endsWith(")") ? raw.slice(cut + 2, -1) : raw.slice(cut + 2);
  return { path: raw.slice(0, cut), error };
});
</script>

<template>
  <div class="stack page--wide">
    <div class="spread">
      <div>
        <h1>{{ messages.result.title }}</h1>
      </div>
      <div class="row-wrap">
        <button class="btn" @click="openSystemTrash">{{ messages.result.openTrash }}</button>
        <button class="btn" @click="goTo('history')">{{ messages.result.openLog }}</button>
        <button class="btn btn--primary" :disabled="executing" @click="startScan(projectPath)">{{ messages.common.rescan }}</button>
      </div>
    </div>

    <p v-if="!desktop" class="notice">{{ messages.preview.notice }}</p>
    <p v-if="executeErrorMessage" class="notice notice--unsafe">{{ executeErrorMessage }}</p>
    <p v-if="trashOpenFailed" class="notice notice--unsafe">{{ messages.errors.openTrash }}</p>
    <p v-if="logFailure" class="notice notice--unsafe">
      <strong>{{ messages.result.logFailedTitle }}</strong>
      {{ messages.errors.logWrite(logFailure.path) }}
    </p>
    <p v-if="skippedTrashMoves > 0 && resultSummary.trashed === 0" class="notice">
      {{ messages.result.noTrashNotice }}
    </p>
    <p v-if="resultSummary.removed > 0" class="notice">
      {{ messages.result.worktreePermanentNotice(resultSummary.removed) }}
    </p>

    <div
      v-if="lastResult.length > 0"
      class="result-hero"
      :class="resultSummary.failed === 0 ? 'result-hero--success' : 'result-hero--partial'"
    >
      <div class="result-hero__mark" aria-hidden="true">{{ resultSummary.failed === 0 ? "✓" : "!" }}</div>
      <div class="result-hero__copy">
        <h2>{{ resultSummary.failed === 0 ? messages.result.successTitle : messages.result.partialTitle }}</h2>
        <p v-if="resultSummary.failed === 0">
          {{
            messages.result.successSummary(
              resultSummary.removed,
              formatBytes(resultSummary.freed, { approximate: resultSummary.freed_unknown > 0 }),
              resultSummary.trashed,
              formatBytes(resultSummary.trash_bytes, { approximate: resultSummary.trash_unknown > 0 })
            )
          }}
        </p>
        <p v-else>
          {{
            messages.result.partialSummary(
              resultSummary.removed,
              formatBytes(resultSummary.freed, { approximate: resultSummary.freed_unknown > 0 }),
              resultSummary.trashed,
              formatBytes(resultSummary.trash_bytes, { approximate: resultSummary.trash_unknown > 0 }),
              resultSummary.failed
            )
          }}
        </p>
        <p
          v-if="resultSummary.reclaimable > 0 || resultSummary.reclaimable_unknown > 0"
          class="result-hero__total faint tiny"
        >
          {{
            messages.result.totalAfterEmptying(
              formatBytes(resultSummary.reclaimable, {
                approximate: resultSummary.reclaimable_unknown > 0
              })
            )
          }}
        </p>
        <div v-if="topItems.length > 0" class="result-hero__top">
          <div class="result-hero__top-title">
            {{ showingRemaining ? messages.result.topRemaining : messages.result.topHandled }}
          </div>
          <ul class="result-hero__top-list">
            <li v-for="item in topItems" :key="`${item.target}-${item.action}`">
              <span class="result-hero__top-path mono">{{ displayPath(item.target, root) }}</span>
              <span class="result-hero__top-size numeric">{{ formatBytes(item.size_bytes) }}</span>
              <span class="outcome" :class="`outcome--${item.result}`">{{ outcomeLabel(item.result) }}</span>
            </li>
          </ul>
        </div>
      </div>
    </div>

    <div class="stat-grid stat-grid--4">
      <StatCard
        :label="messages.result.stats.freedNow"
        tone="accent"
        :value="formatBytes(resultSummary.freed, { approximate: resultSummary.freed_unknown > 0 })"
        :sub="resultSummary.freed_unknown > 0 ? messages.result.stats.freedNowSubUnknown(resultSummary.freed_unknown) : messages.result.stats.freedNowSub"
      />
      <StatCard
        :label="messages.result.stats.inTrash"
        :value="formatBytes(resultSummary.trash_bytes, { approximate: resultSummary.trash_unknown > 0 })"
        :sub="resultSummary.trash_unknown > 0 ? messages.result.stats.inTrashSubUnknown(resultSummary.trash_unknown) : messages.result.stats.inTrashSub(resultSummary.trashed)"
      />
      <StatCard :label="messages.result.stats.handled" tone="safe" value-tone :value="String(okCount)" :sub="messages.result.stats.handledSub(resultSummary.removed, resultSummary.trashed)" />
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
              <th class="cell-action">{{ messages.result.columns.action }}</th>
              <th class="cell-target">{{ messages.result.columns.target }}</th>
              <th class="cell-detail">{{ messages.result.columns.detail }}</th>
              <th class="cell-outcome cell-outcome-cell">{{ messages.result.columns.result }}</th>
              <th class="cell-size cell-bytes">{{ messages.result.columns.size }}</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="(item, index) in lastResult"
              :key="`${item.target}-${index}`"
              :class="item.result === 'ok' ? 'result-row--removed' : 'result-row--kept'"
              :style="{ animationDelay: `${Math.min(index, 24) * 45}ms` }"
            >
              <td class="nowrap tiny cell-action">{{ actionLabel(item.action) }}</td>
              <td class="cell-target">
                <div class="row-wrap">
                  <span class="mono">{{ displayPath(item.target, root) }}</span>
                  <RevealPathButton v-if="item.result !== 'ok' && item.action !== 'prune'" :path="item.target" compact />
                </div>
              </td>
              <td class="tiny faint cell-detail">{{ detailText(item.detail_code, item.detail_args, item.detail) }}</td>
              <td class="cell-outcome cell-outcome-cell">
                <span class="outcome" :class="`outcome--${item.result}`">{{ outcomeLabel(item.result) }}</span>
              </td>
              <td class="cell-size cell-bytes numeric">{{ formatBytes(item.size_bytes) }}</td>
            </tr>
            <tr v-if="lastResult.length === 0">
              <td colspan="5"><div class="empty">{{ messages.result.empty }}</div></td>
            </tr>
          </tbody>
        </table>
      </div>
      <div class="card__foot">
        <span class="faint tiny">
          {{ " " + messages.result.trashNotice + " " + messages.result.footer + " " }}
        </span>
      </div>
    </div>

    <div class="actionbar">
      <div class="actionbar__side">
        <button class="btn" data-nav-back="plan" @click="goTo('plan')">{{ messages.plan.title }}</button>
      </div>
      <span class="actionbar__summary muted">{{ messages.result.complete }}</span>
      <div class="actionbar__side" style="margin-left: auto">
        <button class="btn" @click="goTo('history')">{{ messages.result.viewHistory }}</button>
        <button class="btn btn--primary" @click="goTo('overview')">
          {{ messages.result.backToOverview }}
        </button>
      </div>
    </div>
  </div>
</template>
