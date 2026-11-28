<script setup lang="ts">
import { computed, ref } from "vue";
import type { Reason, SafetyStatus } from "../types";
import { sortReasons } from "../state";
import { reasonText } from "../format";
import { messages } from "../i18n";

const props = defineProps<{ reasons: Reason[]; limit?: number }>();

const MARK: Record<SafetyStatus, string> = { Safe: "✓", Review: "!", Unsafe: "✕" };

// Every reason, most severe first. A row has a line of its own for the evidence
// now, so nothing is gained by hiding reasons on the ordinary list pages — the
// reasons are what the verdict was derived from, so the ones behind it are what a
// reader checking that verdict needs. The clone card is the exception: a clone
// blocker repeats one sentence per linked worktree, so it passes `limit` and keeps
// the remaining rows behind a control.
const expanded = ref(false);
const sorted = computed(() => sortReasons(props.reasons));
const shown = computed(() =>
  props.limit !== undefined && !expanded.value ? sorted.value.slice(0, props.limit) : sorted.value
);
const hidden = computed(() =>
  props.limit === undefined ? 0 : Math.max(0, sorted.value.length - props.limit)
);
</script>

<template>
  <div class="reasons">
    <div
      v-for="(reason, index) in shown"
      :key="`${reason.code}-${reason.evidence}-${index}`"
      class="reason"
      :class="`reason--${reason.severity.toLowerCase()}`"
    >
      <span class="reason__mark">{{ MARK[reason.severity] }}</span>
      <span class="reason__text">{{ reasonText(reason) }}</span>
      <span v-if="reason.evidence" class="reason__evidence">{{ reason.evidence }}</span>
    </div>
    <button v-if="hidden > 0" type="button" class="reason-toggle" @click="expanded = !expanded">
      {{ expanded ? messages.common.showLess : messages.common.showMore(hidden) }}
    </button>
  </div>
</template>
