<script setup lang="ts">
import type { SafetyStatus } from "../types";
import { statusLabel, statusShort } from "../types";

withDefaults(defineProps<{ status: SafetyStatus; compact?: boolean }>(), { compact: false });

// Read through functions, not maps built here: the message table is rebound when
// the language changes, and a map captured at import time would never see it.
</script>

<template>
  <span class="badge" :class="`badge--${status.toLowerCase()}`" :title="statusLabel(status)">
    <span class="dot" :class="`dot--${status.toLowerCase()}`" />
    {{ compact ? statusShort(status) : statusLabel(status) }}
  </span>
</template>
