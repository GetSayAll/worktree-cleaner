<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { recentProjects, scanError, startScan } from "../state";
import { homePath } from "../format";
import { messages } from "../i18n";
import { isDesktop, onPathsDropped, pickDirectory } from "../platform";

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
// `dragActive` drives nothing but the highlight, so a dropped path arriving
// without a `dragleave` cannot leave the panel stuck in the active look.
const dropStyle = computed(() =>
  dragActive.value ? { borderColor: "var(--accent)", background: "var(--accent-weak)" } : undefined
);

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

function scannedAt(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "";
  const minutes = Math.floor((Date.now() - then) / 60000);
  if (minutes < 1) return messages.time.justNow;
  if (minutes < 60) return messages.time.hoursAgo(Math.max(1, Math.round(minutes / 60)));
  const days = Math.round(minutes / 1440);
  if (days <= 1) return messages.time.oneDayAgo;
  if (days < 30) return messages.time.daysAgo(days);
  if (days < 365) return messages.time.monthsAgo(Math.round(days / 30));
  return messages.time.yearsAgo(Math.round(days / 365));
}
</script>

<template>
  <div class="welcome">
    <div class="welcome__hero">
      <p class="faint tiny">{{ messages.welcome.eyebrow }}</p>
      <h1 class="welcome__title">{{ messages.welcome.title }}</h1>
      <p class="welcome__lede">{{ " " + messages.welcome.lede + " " }}</p>
    </div>

    <div
      class="dropzone"
      :style="dropStyle"
      @dragover.prevent="dragActive = true"
      @dragleave="dragActive = false"
      @drop.prevent="onDrop"
    >
      <div class="pick-row">
        <input
          v-model="path"
          class="input input--mono"
          :placeholder="messages.welcome.pathPlaceholder"
          @keyup.enter="submit"
        />
        <button
          class="btn"
          data-action="choose-folder"
          :title="desktop ? messages.welcome.chooseFolderTitle : messages.welcome.chooseFolderUnavailable"
          @click="chooseFolder"
        >
          {{ messages.welcome.chooseFolder }}
        </button>
        <button
          class="btn btn--primary"
          data-action="start-scan"
          :disabled="!path.trim()"
          @click="submit"
        >
          {{ messages.welcome.startScan }}
        </button>
      </div>
      <p v-if="pickerUnavailable" class="field__hint field__hint--warn">
        {{ messages.welcome.chooseFolderUnavailable }}
      </p>
      <p v-if="pickerError" class="field__hint field__hint--warn">
        {{ messages.welcome.pickerFailed(pickerError) }}
      </p>
      <p class="field__hint">{{ messages.welcome.dropHint }}</p>
    </div>

    <p v-if="scanError" class="notice notice--unsafe">{{ messages.welcome.scanFailed(scanError) }}</p>

    <div class="promise-grid">
      <div class="promise">
        <div class="promise__k">{{ messages.welcome.promises.readOnlyTitle }}</div>
        <div class="promise__v">{{ messages.welcome.promises.readOnlyBody }}</div>
      </div>
      <div class="promise">
        <div class="promise__k">{{ messages.welcome.promises.evidenceTitle }}</div>
        <div class="promise__v">{{ messages.welcome.promises.evidenceBody }}</div>
      </div>
      <div class="promise">
        <div class="promise__k">{{ messages.welcome.promises.trashTitle }}</div>
        <div class="promise__v">{{ messages.welcome.promises.trashBody }}</div>
      </div>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>{{ messages.welcome.recentTitle }}</h2>
        <span class="faint tiny">{{ messages.welcome.recentHint }}</span>
      </div>
      <div class="card__body">
        <p v-if="recentProjects.length === 0" class="faint">{{ messages.welcome.recentEmpty }}</p>
        <div v-else class="recent-list">
          <button
            v-for="item in recentProjects"
            :key="item.path"
            class="recent"
            @click="startScan(item.path)"
          >
            <span class="recent__path">{{ homePath(item.path) }}</span>
            <span class="faint tiny nowrap">
              {{
                item.scanned_at
                  ? messages.welcome.lastScan(scannedAt(item.scanned_at))
                  : messages.welcome.neverScanned
              }}
            </span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
