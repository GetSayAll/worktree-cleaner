<script setup lang="ts">
import { computed, ref } from "vue";
import { startScan } from "../state";
import { mockRoot } from "../mock/snapshot";
import { formatBytes, homePath } from "../format";

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
      <p class="faint tiny">Worktree Cleaner</p>
      <h1 class="welcome__title">Project cleanup audit</h1>
      <p class="welcome__lede">
        Scan a repository, list all of its Git worktrees and build artifacts, and explain item by item why each one can or cannot be deleted. Scanning is read-only by default; deletions always go to the system Trash.
      </p>
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
          placeholder="/Users/andy/MySrc/your-repo"
          @keyup.enter="submit"
        />
        <button class="btn" title="In the prototype, fills in a sample path; the shipped version uses a system folder picker" @click="fillDemoPath">Choose folder</button>
        <button class="btn btn--primary" :disabled="!path.trim()" @click="submit">Start scan</button>
      </div>
      <p class="field__hint">You can also drag a project folder into this area. The scan phase writes no files.</p>
    </div>

    <div class="promise-grid">
      <div class="promise">
        <div class="promise__k">Read-only scan</div>
        <div class="promise__v">The check phase runs only read-only Git commands; it does not modify the repository or write into the target directory.</div>
      </div>
      <div class="promise">
        <div class="promise__k">Verifiable reasons</div>
        <div class="promise__v">Every judgment comes with evidence: a concrete command, file count, or path. "Looks like junk" is not accepted.</div>
      </div>
      <div class="promise">
        <div class="promise__k">Trash-based deletion</div>
        <div class="promise__v">Directory deletions always go to the system Trash; worktrees go through git worktree remove. Nothing is permanently deleted.</div>
      </div>
    </div>

    <div class="card">
      <div class="card__head">
        <h2>Recent projects</h2>
        <span class="faint tiny">Click to scan</span>
      </div>
      <div class="card__body">
        <div class="recent-list">
          <button v-for="item in recents" :key="item.path" class="recent" @click="startScan(item.path)">
            <span class="recent__path">{{ homePath(item.path) }}</span>
            <span class="faint tiny nowrap">Last scan {{ item.scanned }}</span>
            <span class="row-wrap nowrap">
              <span class="tiny" style="color: var(--safe)">{{ item.safe }}</span>
              <span class="tiny" style="color: var(--review)">{{ item.review }}</span>
              <span class="tiny" style="color: var(--unsafe)">{{ item.unsafe }}</span>
            </span>
            <span class="faint tiny nowrap">Can free {{ formatBytes(item.freed) }}</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
