<script setup lang="ts">
import { computed, ref } from "vue";
import { history } from "../state";
import { formatBytes, formatTimestamp, homePath } from "../format";

type FilterKey = "all" | "worktree-remove" | "trash-move";

const filter = ref<FilterKey>("all");
const copied = ref(false);

const ACTION_LABEL: Record<string, string> = {
  "worktree-remove": "Remove Worktree",
  "trash-move": "Move to Trash",
  prune: "Clean up metadata"
};

const OUTCOME_LABEL: Record<string, string> = { ok: "Succeeded", skipped: "Skipped", failed: "Failed" };

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
        <h1>History</h1>
        <p class="faint tiny">Logs are stored locally and never uploaded. See Settings for the location.</p>
      </div>
      <button class="btn" @click="copyAll">{{ copied ? "Copied" : "Export log" }}</button>
    </div>

    <div class="row-wrap">
      <div class="segmented">
        <button :class="filter === 'all' ? 'is-active' : ''" @click="filter = 'all'">All {{ history.length }}</button>
        <button :class="filter === 'worktree-remove' ? 'is-active' : ''" @click="filter = 'worktree-remove'">
          Remove Worktree
        </button>
        <button :class="filter === 'trash-move' ? 'is-active' : ''" @click="filter = 'trash-move'">Move to Trash</button>
      </div>
    </div>

    <div class="card">
      <div class="card__body card__body--flush">
        <div class="timeline">
          <div v-for="(row, index) in filtered" :key="`${row.at}-${index}`" class="timeline__row">
            <div class="timeline__time">{{ formatTimestamp(row.at) }}</div>
            <div class="timeline__body">
              <div class="row-wrap">
                <span class="chip">{{ ACTION_LABEL[row.action] || row.action }}</span>
                <span class="mono">{{ homePath(row.target) }}</span>
                <span class="outcome" :class="`outcome--${row.result}`">{{ OUTCOME_LABEL[row.result] }}</span>
                <span class="faint tiny numeric">{{ formatBytes(row.size_bytes) }}</span>
              </div>
              <div class="tiny faint">{{ row.reason }}</div>
              <div class="tiny muted">{{ row.detail }}</div>
            </div>
          </div>
          <div v-if="filtered.length === 0" class="empty">No records</div>
        </div>
      </div>
    </div>
  </div>
</template>
