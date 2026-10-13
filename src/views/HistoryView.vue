<script setup lang="ts">
import { computed, ref } from "vue";
import { activity, history, historyError } from "../state";
import { activityText, detailText, formatBytes, formatTimestamp, homePath, reasonCopy } from "../format";
import type { ActivityEvent, HistoryEntry } from "../types";
import { messages } from "../i18n";

type FilterKey = "all" | "worktree-remove" | "trash-move" | "activity";

/**
 * One line of the history, whichever log it came from.
 *
 * The two records stay in their own files and keep their own shapes; the page
 * puts them on one timeline because that is the question being asked — "what
 * happened on this machine" — and a reader should not have to know that the
 * answers live in two places.
 */
type TimelineRow =
  | { kind: "deletion"; at: string; entry: HistoryEntry }
  | { kind: "action"; at: string; event: ActivityEvent };

const filter = ref<FilterKey>("all");
const copied = ref(false);

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

/**
 * The verdict one deletion was based on.
 *
 * A code the reason table knows is rendered from it; a record written before the
 * codes existed keeps its own sentence rather than losing the reason entirely.
 */
function reasonOf(row: HistoryEntry): string {
  const copy = row.reason_code ? reasonCopy(row.reason_code) : undefined;
  return copy ?? row.reason ?? messages.detail.legacy;
}

const filtered = computed<TimelineRow[]>(() => {
  const deletions: TimelineRow[] = history.value
    .filter((row) => {
      if (filter.value === "all") return true;
      // "Your actions" is the other log entirely, not a slice of this one.
      if (filter.value === "activity") return false;
      return row.action === filter.value;
    })
    .map((entry) => ({ kind: "deletion", at: entry.at, entry }));
  const actions: TimelineRow[] =
    filter.value === "all" || filter.value === "activity"
      ? activity.value.map((event) => ({ kind: "action", at: event.at, event }) as TimelineRow)
      : [];
  return [...deletions, ...actions].sort((a, b) => b.at.localeCompare(a.at));
});

async function copyAll() {
  try {
    await navigator.clipboard.writeText(JSON.stringify(history.value, null, 2));
    copied.value = true;
    window.setTimeout(() => (copied.value = false), 1800);
  } catch {
    copied.value = false;
  }
}
</script>

<template>
  <div class="stack page--wide">
    <div class="spread">
      <div>
        <h1>{{ messages.history.title }}</h1>
        <p class="faint tiny">{{ messages.history.lede }}</p>
      </div>
      <button class="btn" @click="copyAll">{{ copied ? messages.common.copied : messages.history.copyLog }}</button>
    </div>

    <p v-if="historyError" class="notice notice--unsafe">{{ messages.history.readFailed(historyError) }}</p>

    <div class="row-wrap">
      <div class="segmented">
        <button :class="filter === 'all' ? 'is-active' : ''" @click="filter = 'all'">{{ messages.history.filterAll(history.length) }}</button>
        <button :class="filter === 'worktree-remove' ? 'is-active' : ''" @click="filter = 'worktree-remove'">
          {{ " " + messages.history.filterWorktreeRemove + " " }}
        </button>
        <button :class="filter === 'trash-move' ? 'is-active' : ''" @click="filter = 'trash-move'">{{ messages.history.filterTrashMove }}</button>
        <button :class="filter === 'activity' ? 'is-active' : ''" @click="filter = 'activity'">{{ messages.activity.filterAction }}</button>
      </div>
    </div>

    <div class="card">
      <div class="card__body card__body--flush">
        <div class="timeline">
          <div v-for="(row, index) in filtered" :key="`${row.at}-${index}`" class="timeline__row">
            <div class="timeline__time">{{ formatTimestamp(row.at) }}</div>
            <div v-if="row.kind === 'action'" class="timeline__body">
              <div class="row-wrap">
                <span class="chip">{{ messages.activity.kinds.action }}</span>
                <span class="tiny">{{ activityText(row.event.action, row.event.args) }}</span>
                <span v-if="row.event.target" class="mono faint tiny">{{ homePath(row.event.target) }}</span>
              </div>
            </div>
            <div v-else class="timeline__body">
              <div class="row-wrap">
                <span class="chip">{{ messages.activity.kinds.deletion }}</span>
                <span class="chip">{{ actionLabel(row.entry.action) }}</span>
                <span class="mono">{{ homePath(row.entry.target) }}</span>
                <span class="outcome" :class="`outcome--${row.entry.result}`">{{ outcomeLabel(row.entry.result) }}</span>
                <span class="faint tiny numeric">{{ formatBytes(row.entry.size_bytes) }}</span>
              </div>
              <div class="tiny faint">{{ reasonOf(row.entry) }}</div>
              <div class="tiny muted">{{ detailText(row.entry.detail_code, row.entry.detail_args, row.entry.detail) }}</div>
            </div>
          </div>
          <div v-if="filtered.length === 0" class="empty">{{ messages.history.empty }}</div>
        </div>
      </div>
    </div>
  </div>
</template>
