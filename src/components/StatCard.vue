<script setup lang="ts">
withDefaults(
  defineProps<{
    label: string;
    value: string;
    sub?: string;
    tone?: "safe" | "review" | "unsafe" | "accent" | "plain";
    valueTone?: boolean;
  }>(),
  { sub: "", tone: "plain", valueTone: false }
);
</script>

<template>
  <div class="stat" :class="tone !== 'plain' ? `stat--${tone}` : ''">
    <div class="stat__label">{{ label }}</div>
    <div class="stat__value" :class="valueTone ? `stat__value--${tone}` : ''">{{ value }}</div>
    <div v-if="sub" class="stat__sub">{{ sub }}</div>
  </div>
</template>
