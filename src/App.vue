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
  selectedSummary,
  snapshot,
  startScan,
  view,
  worktrees,
  type ViewKey
} from "./state";
import { homePath } from "./format";
import { messages } from "./i18n";
import WelcomeView from "./views/WelcomeView.vue";
import ScanningView from "./views/ScanningView.vue";
import OverviewView from "./views/OverviewView.vue";
import WorktreesView from "./views/WorktreesView.vue";
import ArtifactsView from "./views/ArtifactsView.vue";
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
  plan: CleanupPlanView,
  result: ResultView,
  history: HistoryView,
  settings: SettingsView
};

const currentView = computed(() => VIEWS[view.value]);

const NAV: Array<{ key: ViewKey; label: string }> = [
  { key: "welcome", label: messages.nav.project },
  { key: "overview", label: messages.nav.overview },
  { key: "worktrees", label: messages.nav.worktrees },
  { key: "artifacts", label: messages.nav.artifacts },
  { key: "plan", label: messages.nav.plan },
  { key: "result", label: messages.nav.result },
  { key: "history", label: messages.nav.history },
  { key: "settings", label: messages.nav.settings }
];

const totals = computed(() => ({
  safe: counts.value.worktree.Safe + counts.value.artifact.Safe,
  review: counts.value.worktree.Review + counts.value.artifact.Review,
  unsafe: counts.value.worktree.Unsafe + counts.value.artifact.Unsafe
}));

const hasSnapshot = computed(() => snapshot.value !== null);

function navCount(key: ViewKey): string {
  switch (key) {
    case "worktrees":
      return `${worktrees.value.length}`;
    case "artifacts":
      return `${artifacts.value.length}`;
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
        <div class="brand__sub">{{ messages.app.tagline }}</div>
      </div>

      <nav class="nav">
        <button
          v-for="item in NAV"
          :key="item.key"
          class="nav__item"
          :class="view === item.key ? 'is-active' : ''"
          :disabled="!canOpen(item.key)"
          @click="goTo(item.key)"
        >
          <span>{{ item.label }}</span>
          <span class="nav__count">{{ navCount(item.key) }}</span>
        </button>
      </nav>

      <div class="sidebar__foot">
        <p class="sidebar__note">{{ messages.app.readOnlyNote }}</p>
        <p class="sidebar__note">{{ messages.app.versionNote }}</p>
      </div>
    </aside>

    <main class="main">
      <header class="topbar">
        <template v-if="hasSnapshot">
          <span class="topbar__path" :title="projectPath">{{ homePath(projectPath) }}</span>
          <span v-if="isDemoData" class="badge badge--accent">{{ messages.app.sampleData }}</span>
          <div class="topbar__group">
            <span class="badge badge--safe">{{ messages.status.short.Safe }} {{ totals.safe }}</span>
            <span class="badge badge--review">{{ messages.status.short.Review }} {{ totals.review }}</span>
            <span class="badge badge--unsafe">{{ messages.status.short.Unsafe }} {{ totals.unsafe }}</span>
          </div>
          <button class="btn" @click="rescan">{{ messages.common.rescan }}</button>
          <button class="btn" @click="resetToWelcome">{{ messages.app.switchProject }}</button>
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
