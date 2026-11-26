<script setup lang="ts">
import { ref } from "vue";
import { messages } from "../i18n";
import { isDesktop, revealInFileManager } from "../platform";

const props = defineProps<{ path: string; compact?: boolean }>();
const desktop = isDesktop();
const failed = ref(false);

async function reveal() {
  failed.value = false;
  try {
    await revealInFileManager(props.path);
  } catch {
    failed.value = true;
  }
}
</script>

<template>
  <span v-if="desktop" class="reveal-path">
    <button
      type="button"
      class="btn btn--ghost"
      :class="{ 'reveal-path__compact': compact }"
      :title="failed ? messages.errors.reveal : messages.common.revealHint"
      :aria-label="messages.common.revealHint"
      @click="reveal"
    >
      {{ compact ? messages.common.revealShort : messages.common.reveal }}
    </button>
    <span v-if="failed && !compact" class="tiny faint reveal-path__error" role="status">
      {{ messages.errors.reveal }}
    </span>
  </span>
</template>
