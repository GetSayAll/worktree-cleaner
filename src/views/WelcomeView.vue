<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { recentProjects, scanError, startScan } from "../state";
import { formatRelTime, homePath } from "../format";
import { messages } from "../i18n";
import { isDesktop, onPathsDropped, pickDirectory } from "../platform";
import AppIcon from "../components/AppIcon.vue";
import brandLogo from "../assets/brand.svg";
import EmptyState from "../components/EmptyState.vue";

const path = ref("");
const dragActive = ref(false);
const pickerUnavailable = ref(false);
const pickerError = ref<string | null>(null);
const desktop = isDesktop();

let stopDropListener: (() => void) | null = null;

onMounted(async () => {
  // The desktop shell hands over real paths, which is more useful than the
  // browser's File objects (a directory drop gives no usable path there).
  stopDropListener = await onPathsDropped((paths) => {
    const first = paths[0];
    if (first) path.value = first;
  });
});

onUnmounted(() => stopDropListener?.());

function submit() {
  if (!path.value.trim()) return;
  void startScan(path.value);
}

async function chooseFolder() {
  pickerError.value = null;
  try {
    const picked = await pickDirectory(messages.welcome.chooseFolderTitle);
    if (picked !== null) {
      pickerUnavailable.value = false;
      path.value = picked;
      return;
    }
    // `null` means the user cancelled *or* there was no picker to open. Only the
    // second case needs saying out loud, and the field stays the way in.
    pickerUnavailable.value = true;
  } catch (error) {
    // The picker can fail on its own — no permission, no display, a plugin that
    // is not registered. A click that raises silently is the exact failure this
    // button was rewritten to remove, so the reason goes on the page.
    pickerUnavailable.value = false;
    pickerError.value = error instanceof Error ? error.message : String(error);
  }
}

/**
 * The drop event the browser gives us.
 *
 * Tauri intercepts the real one (see `onPathsDropped`), so this only has to keep
 * the panel from navigating away; the value arrives through the listener above.
 */
function onDrop(event: DragEvent) {
  dragActive.value = false;
  const text = event.dataTransfer?.getData("text/plain") ?? "";
  if (text.trim()) path.value = text.trim();
}
</script>

<template>
  <div class="welcome">
    <header class="welcome__hero">
      <img class="welcome__mark" :src="brandLogo" alt="" />
      <h1 class="welcome__title">{{ messages.welcome.title }}</h1>
      <p class="welcome__lede">{{ messages.welcome.lede }}</p>
    </header>

    <div
      class="composer"
      :class="{ 'is-dragging': dragActive }"
      @dragover.prevent="dragActive = true"
      @dragleave="dragActive = false"
      @drop.prevent="onDrop"
    >
      <div class="composer__row">
        <AppIcon name="folder" class="composer__leading" />
        <input
          v-model="path"
          class="input input--mono composer__input"
          :placeholder="messages.welcome.pathPlaceholder"
          @keyup.enter="submit"
        />
        <div class="composer__actions">
          <button
            class="btn btn--ghost composer__browse"
            data-action="choose-folder"
            :title="desktop ? messages.welcome.chooseFolderTitle : messages.welcome.chooseFolderUnavailable"
            @click="chooseFolder"
          >
            {{ messages.welcome.chooseFolder }}
          </button>
          <button
            class="btn btn--primary btn--cta"
            data-action="start-scan"
            :disabled="!path.trim()"
            @click="submit"
          >
            {{ messages.welcome.startScan }}
          </button>
        </div>
      </div>
      <p v-if="pickerUnavailable" class="field__hint field__hint--warn composer__message">
        {{ messages.welcome.chooseFolderUnavailable }}
      </p>
      <p v-if="pickerError" class="field__hint field__hint--warn composer__message">
        {{ messages.welcome.pickerFailed(pickerError) }}
      </p>
      <p class="field__hint composer__hint">{{ messages.welcome.dropHint }}</p>
    </div>

    <p v-if="scanError" class="notice notice--unsafe">{{ messages.welcome.scanFailed(scanError) }}</p>

    <div class="assurances">
      <div class="assurance">
        <AppIcon name="lock" class="assurance__icon" />
        <div class="assurance__text">
          <p class="assurance__title">{{ messages.welcome.promises.readOnlyTitle }}</p>
          <p class="assurance__body">{{ messages.welcome.promises.readOnlyBody }}</p>
        </div>
      </div>
      <div class="assurance">
        <AppIcon name="evidence" class="assurance__icon" />
        <div class="assurance__text">
          <p class="assurance__title">{{ messages.welcome.promises.evidenceTitle }}</p>
          <p class="assurance__body">{{ messages.welcome.promises.evidenceBody }}</p>
        </div>
      </div>
      <div class="assurance">
        <AppIcon name="trash" class="assurance__icon" />
        <div class="assurance__text">
          <p class="assurance__title">{{ messages.welcome.promises.trashTitle }}</p>
          <p class="assurance__body">{{ messages.welcome.promises.trashBody }}</p>
        </div>
      </div>
    </div>

    <section class="recent-panel">
      <div class="recent-panel__head">
        <h2>{{ messages.welcome.recentTitle }}</h2>
        <span class="faint tiny">{{ messages.welcome.recentHint }}</span>
      </div>
      <EmptyState v-if="recentProjects.length === 0" compact icon="folder" :text="messages.welcome.recentEmpty" />
      <div v-else class="recent-list">
        <button
          v-for="item in recentProjects"
          :key="item.path"
          class="recent"
          @click="startScan(item.path)"
        >
          <AppIcon name="folder" class="recent__icon" />
          <span class="recent__path">{{ homePath(item.path) }}</span>
          <span class="recent__time">
            {{
              item.scanned_at
                ? messages.welcome.lastScan(formatRelTime(item.scanned_at))
                : messages.welcome.neverScanned
            }}
          </span>
          <AppIcon name="chevron" class="recent__chevron" />
        </button>
      </div>
    </section>
  </div>
</template>
