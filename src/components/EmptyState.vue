<script setup lang="ts">
import AppIcon from "./AppIcon.vue";
import type { IconName } from "./icons";

withDefaults(
  defineProps<{
    text: string;
    icon?: IconName;
    compact?: boolean;
  }>(),
  { icon: "folder", compact: false }
);
</script>

<template>
  <div class="empty" :class="{ 'empty--compact': compact }">
    <span class="empty__icon"><AppIcon :name="icon" /></span>
    <p>{{ text }}</p>
  </div>
</template>
