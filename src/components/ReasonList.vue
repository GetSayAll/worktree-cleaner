<script setup lang="ts">
import { computed } from "vue";
import type { Reason, SafetyStatus } from "../types";
import { sortReasons } from "../state";

const props = withDefaults(defineProps<{ reasons: Reason[]; limit?: number }>(), { limit: 0 });

const MARK: Record<SafetyStatus, string> = { Safe: "✓", Review: "!", Unsafe: "✕" };

const shown = computed(() => {
  const sorted = sortReasons(props.reasons);
  return props.limit > 0 ? sorted.slice(0, props.limit) : sorted;
});
</script>

<template>
  <div class="reasons">
    <div v-for="reason in shown" :key="reason.code + reason.text" class="reason" :class="`reason--${reason.severity.toLowerCase()}`">
      <span class="reason__mark" :class="`gate__mark--${reason.severity === 'Safe' ? 'ok' : 'bad'}`">{{ MARK[reason.severity] }}</span>
      <span>
        <span class="reason__text">{{ reason.text }}</span>
        <span v-if="reason.evidence" class="reason__evidence">{{ reason.evidence }}</span>
      </span>
    </div>
  </div>
</template>
