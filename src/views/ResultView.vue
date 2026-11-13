<script setup lang="ts">
import { computed, ref } from "vue";
import { executeError, goTo, lastResult, logError, projectPath, resultSummary, snapshot, startScan } from "../state";
import StatCard from "../components/StatCard.vue";
import AppIcon from "../components/AppIcon.vue";
import EmptyState from "../components/EmptyState.vue";
import { detailText, formatBytes, displayPath } from "../format";
import { messages } from "../i18n";
import { openTrash, isDesktop } from "../platform";

const desktop = isDesktop();

const root = computed(() => snapshot.value?.root ?? "");

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

/**
 * What the bar under the result says.
 *
 * The count is the message: "cleanup complete" on a run where every item was
 * skipped is the one sentence that made a safe no-op look like a deletion. The
 * bar now names the handled and not-executed counts, and the page carries a
 * banner when nothing was removed at all.
 */
const finishedSummary = computed(() => {
  const total = lastResult.value.length;
  const handled = okCount.value;
  const notDeleted = total - handled;
  if (total === 0) return messages.result.finishedEmpty;
  if (handled === 0) return messages.result.finishedNone(notDeleted);
  if (notDeleted === 0) return messages.result.finishedAll(handled);
  return messages.result.finishedPartial(handled, notDeleted);
});

/** The one banner at the top of the page: what the run actually did. */
const outcome = computed<{
  tone: "banner--safe" | "banner--warn";
  icon: "check" | "alert";
  title: string;
  body: string;
} | null>(() => {
  const total = lastResult.value.length;
  if (total === 0) return null;
  const handled = okCount.value;
  const notDeleted = total - handled;
  if (handled === 0) {
    return {
      tone: "banner--warn",
      icon: "alert",
      title: messages.result.outcomeNoneTitle,
      body: messages.result.outcomeNoneBody(notDeleted)
    };
  }
  if (notDeleted > 0) {
    return {
      tone: "banner--warn",
      icon: "alert",
      title: messages.result.outcomePartialTitle,
      body: messages.result.outcomePartialBody(handled, notDeleted)
    };
  }
  return {
    tone: "banner--safe",
    icon: "check",
    title: messages.result.outcomeDoneTitle,
    body: messages.result.outcomeDoneBody(handled)
  };
});

/**
 * The failure of a system action this page offers.
 *
 * Opening the Trash or the log can fail — the path may not exist, the opener may
 * refuse. None of those should leave a click with no effect and no explanation.
 */
const systemError = ref<string | null>(null);

async function openSystemTrash() {
  systemError.value = null;
  try {
    await openTrash();
  } catch (error) {
    systemError.value = error instanceof Error ? error.message : String(error);
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
    <div class="page-head">
      <div class="page-head__lead">
        <span class="page-head__icon"><AppIcon name="result" /></span>
        <div class="page-head__text">
          <h1>{{ messages.result.title }}</h1>
          <p class="page-head__sub mono">{{ projectPath }}</p>
        </div>
      </div>
      <div class="page-head__actions">
        <button class="btn" @click="openSystemTrash">{{ messages.result.openTrash }}</button>
        <button class="btn" @click="goTo('history')">{{ messages.result.openLog }}</button>
        <button class="btn" @click="startScan(projectPath)">{{ messages.common.rescan }}</button>
      </div>
    </div>

    <p v-if="!desktop" class="notice">{{ messages.preview.notice }}</p>
    <p v-if="executeError" class="notice notice--unsafe">{{ executeError }}</p>
    <p v-if="systemError" class="notice notice--unsafe">{{ systemError }}</p>
    <p v-if="logFailure" class="notice notice--unsafe">
      <strong>{{ messages.result.logFailedTitle }}</strong>
      {{ messages.result.logFailedBody(logFailure.path, logFailure.error) }}
    </p>

    <div v-if="outcome" class="banner" :class="outcome.tone">
      <AppIcon :name="outcome.icon" class="banner__icon" />
      <span><span class="banner__title">{{ outcome.title }}</span>{{ outcome.body }}</span>
    </div>

    <div class="stat-grid stat-grid--4">
      <StatCard
        :label="messages.result.stats.spaceFreed"
        tone="accent"
        :value="formatBytes(resultSummary.freed, { approximate: resultSummary.freed_unknown > 0 })"
        :sub="resultSummary.freed_unknown > 0 ? messages.result.stats.spaceFreedSubUnknown(resultSummary.freed_unknown) : messages.result.stats.spaceFreedSub"
      />
      <StatCard :label="messages.result.stats.succeeded" tone="safe" value-tone :value="String(okCount)" :sub="messages.result.stats.succeededSub(resultSummary.trashed)" />
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
              <th class="cell-action">{{ messages.result.columns.action }}</th>
              <th class="cell-target">{{ messages.result.columns.target }}</th>
              <th class="cell-detail">{{ messages.result.columns.detail }}</th>
              <th class="cell-outcome cell-outcome-cell">{{ messages.result.columns.result }}</th>
              <th class="cell-size cell-bytes">{{ messages.result.columns.size }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(item, index) in lastResult" :key="`${item.target}-${index}`">
              <td class="nowrap tiny cell-action">{{ actionLabel(item.action) }}</td>
              <td class="mono cell-target">{{ displayPath(item.target, root) }}</td>
              <td class="tiny faint cell-detail">{{ detailText(item.detail_code, item.detail_args, item.detail) }}</td>
              <td class="cell-outcome cell-outcome-cell">
                <span class="outcome" :class="`outcome--${item.result}`">{{ outcomeLabel(item.result) }}</span>
              </td>
              <td class="cell-size cell-bytes numeric">{{ formatBytes(item.size_bytes) }}</td>
            </tr>
            <tr v-if="lastResult.length === 0">
              <td colspan="5"><EmptyState compact icon="result" :text="messages.result.empty" /></td>
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
      <span class="actionbar__summary muted">{{ finishedSummary }}</span>
      <div class="actionbar__side" style="margin-left: auto">
        <button class="btn" @click="goTo('history')">{{ messages.result.viewHistory }}</button>
        <button class="btn btn--primary" @click="goTo('overview')">
          {{ messages.result.backToOverview }}
        </button>
      </div>
    </div>
  </div>
</template>
