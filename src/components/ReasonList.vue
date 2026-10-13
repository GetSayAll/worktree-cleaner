<script setup lang="ts">
import { computed } from "vue";
import type { Reason, SafetyStatus } from "../types";
import { sortReasons } from "../state";

const props = defineProps<{ reasons: Reason[] }>();

const MARK: Record<SafetyStatus, string> = { Safe: "✓", Review: "!", Unsafe: "✕" };

// Every reason, most severe first. A row has a line of its own for the evidence
// now, so nothing is gained by showing one reason and hiding the rest behind a
// tooltip — and the reasons are what the verdict was derived from, so the ones
// behind it are what a reader checking that verdict needs.
const shown = computed(() => sortReasons(props.reasons));
</script>

<template>
  <div class="reasons">
    <div v-for="reason in shown" :key="reason.code + reason.text" class="reason" :class="`reason--${reason.severity.toLowerCase()}`">
      <span class="reason__mark">{{ MARK[reason.severity] }}</span>
      <span class="reason__text">{{ reason.text }}</span>
      <span v-if="reason.evidence" class="reason__evidence">{{ reason.evidence }}</span>
    </div>
  </div>
</template>
