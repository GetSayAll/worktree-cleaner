<script setup lang="ts">
import type { SafetyStatus } from "../types";
import { STATUS_LABEL } from "../types";

withDefaults(defineProps<{ status: SafetyStatus; compact?: boolean }>(), { compact: false });

const SHORT: Record<SafetyStatus, string> = {
  Safe: "Safe",
  Review: "Review",
  Unsafe: "Blocked"
};
</script>

<template>
  <span class="badge" :class="`badge--${status.toLowerCase()}`" :title="STATUS_LABEL[status]">
    <span class="dot" :class="`dot--${status.toLowerCase()}`" />
    {{ compact ? SHORT[status] : STATUS_LABEL[status] }}
  </span>
</template>
