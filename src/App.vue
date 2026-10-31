<script setup lang="ts">
import { computed } from "vue";
import {
  artifacts,
  canOpen,
  counts,
  goTo,
  history,
  isDemoData,
  lastResult,
  projectPath,
  resetToWelcome,
  scanState,
  selectedSummary,
  snapshot,
  startScan,
  view,
  worktrees,
  type ViewKey
} from "./state";
import { homePath } from "./format";
import { messages, type Messages } from "./i18n";
import { isDesktop } from "./platform";
import WelcomeView from "./views/WelcomeView.vue";
import ScanningView from "./views/ScanningView.vue";
import OverviewView from "./views/OverviewView.vue";
import WorktreesView from "./views/WorktreesView.vue";
import ArtifactsView from "./views/ArtifactsView.vue";
import HygieneView from "./views/HygieneView.vue";
import CleanupPlanView from "./views/CleanupPlanView.vue";
import ResultView from "./views/ResultView.vue";
import HistoryView from "./views/HistoryView.vue";
import SettingsView from "./views/SettingsView.vue";

const VIEWS = {
  welcome: WelcomeView,
  scanning: ScanningView,
  overview: OverviewView,
  worktrees: WorktreesView,
  artifacts: ArtifactsView,
  hygiene: HygieneView,
  plan: CleanupPlanView,
  result: ResultView,
  history: HistoryView,
  settings: SettingsView
};

const currentView = computed(() => VIEWS[view.value]);

// Keys, not labels: the language can change while the app is running, so the
// text has to be looked up during rendering rather than captured here.
const NAV: Array<{ key: ViewKey; label: keyof Messages["nav"] }> = [
  { key: "welcome", label: "project" },
  { key: "overview", label: "overview" },
  { key: "worktrees", label: "worktrees" },
  { key: "artifacts", label: "artifacts" },
  { key: "hygiene", label: "hygiene" },
  { key: "plan", label: "plan" },
  { key: "result", label: "result" },
  { key: "history", label: "history" },
  { key: "settings", label: "settings" }
];

const totals = computed(() => ({
  safe: counts.value.worktree.Safe + counts.value.artifact.Safe,
  review: counts.value.worktree.Review + counts.value.artifact.Review,
  unsafe: counts.value.worktree.Unsafe + counts.value.artifact.Unsafe
}));

const hasSnapshot = computed(() => snapshot.value !== null);

const desktop = isDesktop();

function navCount(key: ViewKey): string {
  switch (key) {
    case "worktrees":
      return `${worktrees.value.length}`;
    case "artifacts":
      return `${artifacts.value.length}`;
    case "hygiene": {
      // What is broken, not what was checked: a page whose badge counted 22
      // compliant rules would say nothing a reader needs.
      const broken = snapshot.value?.hygiene.rules.filter((rule) => rule.level === "violation").length ?? 0;
      return broken > 0 ? `${broken}` : "";
    }
    case "plan": {
      const picked = selectedSummary.value.worktrees + selectedSummary.value.artifacts;
      return picked > 0 ? `${picked}` : "";
    }
    case "result":
      return lastResult.value.length > 0 ? `${lastResult.value.length}` : "";
    case "history":
      return `${history.value.length}`;
    default:
      return "";
  }
}

function rescan() {
  startScan(projectPath.value);
}
</script>

<template>
  <div class="app">
    <aside class="sidebar">
      <div class="brand">
        <div class="brand__name">{{ messages.app.name }}</div>
        <div v-if="messages.app.tagline" class="brand__sub">{{ messages.app.tagline }}</div>
      </div>

      <nav class="nav">
        <button
          v-for="item in NAV"
          :key="item.key"
          :data-view="item.key"
          class="nav__item"
          :class="view === item.key ? 'is-active' : ''"
          :disabled="!canOpen(item.key)"
          @click="goTo(item.key)"
        >
          <span>{{ messages.nav[item.label] }}</span>
          <span class="nav__count">{{ navCount(item.key) }}</span>
        </button>
      </nav>

      <div class="sidebar__foot">
        <p class="sidebar__note">{{ messages.app.readOnlyNote }}</p>
        <p v-if="!desktop" class="sidebar__note">{{ messages.app.previewNote }}</p>
        <p class="sidebar__note">{{ messages.app.versionNote }}</p>
      </div>
    </aside>

    <main class="main">
      <header class="topbar">
        <!-- The path is shown whenever there is one, including *during* a scan:
             `projectPath` is set before the scan starts, and a bar that says "no
             project selected" while the app is visibly scanning that project
             reads as a bug. The counts and the actions wait for the result. -->
        <template v-if="projectPath">
          <span class="topbar__path" :title="projectPath">{{ homePath(projectPath) }}</span>
          <span v-if="scanState.running" class="faint tiny">{{ messages.scan.scanning }}</span>
          <span v-if="isDemoData" class="badge badge--accent">{{ messages.app.sampleData }}</span>
          <div v-if="hasSnapshot" class="topbar__group">
            <span class="badge badge--safe">{{ messages.status.short.Safe }} {{ totals.safe }}</span>
            <span class="badge badge--review">{{ messages.status.short.Review }} {{ totals.review }}</span>
            <span class="badge badge--unsafe">{{ messages.status.short.Unsafe }} {{ totals.unsafe }}</span>
          </div>
          <button v-if="hasSnapshot" class="btn" @click="rescan">{{ messages.common.rescan }}</button>
          <button v-if="hasSnapshot" class="btn" @click="resetToWelcome">{{ messages.app.switchProject }}</button>
        </template>
        <template v-else>
          <span class="topbar__path">{{ messages.app.noProject }}</span>
          <span class="faint tiny">{{ messages.app.pickRepository }}</span>
        </template>
      </header>

      <section class="content">
        <component :is="currentView" />
      </section>
    </main>
  </div>
</template>
