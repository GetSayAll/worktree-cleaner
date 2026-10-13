<script setup lang="ts">
import { computed, ref } from "vue";
import { startScan } from "../state";
import { mockRoot } from "../mock/snapshot";
import { formatBytes, homePath } from "../format";
import { messages } from "../i18n";

const path = ref("");
const dragActive = ref(false);

const recents = [
  { path: mockRoot, scanned: "Today 09:12", safe: 8, review: 6, unsafe: 4, freed: 4.2 * 1024 ** 3 },
  {
    path: "/Users/andy/MySrc/private-marketing-toolkit",
    scanned: "3 days ago",
    safe: 3,
    review: 1,
    unsafe: 2,
    freed: 1.1 * 1024 ** 3
  },
  { path: "/Users/andy/MySrc/worktree-cleaner", scanned: "11 days ago", safe: 2, review: 0, unsafe: 1, freed: 612 * 1024 ** 2 }
];

const dropStyle = computed(() =>
  dragActive.value ? { borderColor: "var(--accent)", background: "var(--accent-weak)" } : undefined
);

function submit() {
  if (!path.value.trim()) return;
  startScan(path.value);
}

function fillDemoPath() {
  path.value = mockRoot;
}

function onDrop(event: DragEvent) {
  dragActive.value = false;
  const dropped = event.dataTransfer?.files?.[0] as unknown as { path?: string } | undefined;
  const text = dropped?.path ?? event.dataTransfer?.getData("text/plain") ?? "";
  if (text) path.value = text.trim();
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
        <button class="btn" :title="messages.welcome.chooseFolderTitle" @click="fillDemoPath">{{ messages.welcome.chooseFolder }}</button>
        <button class="btn btn--primary" :disabled="!path.trim()" @click="submit">{{ messages.welcome.startScan }}</button>
      </div>
      <p class="field__hint">{{ messages.welcome.dropHint }}</p>
    </div>

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
        <div class="recent-list">
          <button v-for="item in recents" :key="item.path" class="recent" @click="startScan(item.path)">
            <span class="recent__path">{{ homePath(item.path) }}</span>
            <span class="faint tiny nowrap">{{ messages.welcome.lastScan(item.scanned) }}</span>
            <span class="row-wrap nowrap">
              <span class="tiny" style="color: var(--safe)">{{ item.safe }}</span>
              <span class="tiny" style="color: var(--review)">{{ item.review }}</span>
              <span class="tiny" style="color: var(--unsafe)">{{ item.unsafe }}</span>
            </span>
            <span class="faint tiny nowrap">{{ messages.welcome.canFree(formatBytes(item.freed)) }}</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
