<script setup lang="ts">
import { computed, ref } from "vue";
import { history, historyError } from "../state";
import { formatBytes, formatTimestamp, homePath } from "../format";
import { messages } from "../i18n";

type FilterKey = "all" | "worktree-remove" | "trash-move";

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

const filtered = computed(() => {
  const rows = filter.value === "all" ? history.value : history.value.filter((row) => row.action === filter.value);
  return [...rows].sort((a, b) => b.at.localeCompare(a.at));
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
  <div class="stack">
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
      </div>
    </div>

    <div class="card">
      <div class="card__body card__body--flush">
        <div class="timeline">
          <div v-for="(row, index) in filtered" :key="`${row.at}-${index}`" class="timeline__row">
            <div class="timeline__time">{{ formatTimestamp(row.at) }}</div>
            <div class="timeline__body">
              <div class="row-wrap">
                <span class="chip">{{ actionLabel(row.action) }}</span>
                <span class="mono">{{ homePath(row.target) }}</span>
                <span class="outcome" :class="`outcome--${row.result}`">{{ outcomeLabel(row.result) }}</span>
                <span class="faint tiny numeric">{{ formatBytes(row.size_bytes) }}</span>
              </div>
              <div class="tiny faint">{{ row.reason }}</div>
              <div class="tiny muted">{{ row.detail }}</div>
            </div>
          </div>
          <div v-if="filtered.length === 0" class="empty">{{ messages.history.empty }}</div>
        </div>
      </div>
    </div>
  </div>
</template>
