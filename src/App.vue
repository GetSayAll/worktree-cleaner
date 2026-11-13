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
import { updateState } from "./update";
import AppIcon from "./components/AppIcon.vue";
import brandLogo from "./assets/brand.svg";
import type { IconName } from "./components/icons";
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
type NavLabel = Exclude<keyof Messages["nav"], "groups">;
type NavEntry = { key: ViewKey; label: NavLabel; icon: IconName };

// The rail is read in three passes: the way in, what the scan found, and what can
// be done about it. Grouping nine equal rows into these blocks is what keeps a
// long navigation scannable without hiding anything behind a disclosure.
const NAV_SECTIONS: Array<{ label: keyof Messages["nav"]["groups"] | null; items: NavEntry[] }> = [
  {
    label: null,
    items: [{ key: "welcome", label: "project", icon: "home" }]
  },
  {
    label: "audit",
    items: [
      { key: "overview", label: "overview", icon: "overview" },
      { key: "worktrees", label: "worktrees", icon: "branch" },
      { key: "artifacts", label: "artifacts", icon: "artifacts" },
      { key: "hygiene", label: "hygiene", icon: "hygiene" }
    ]
  },
  {
    label: "cleanup",
    items: [
      { key: "plan", label: "plan", icon: "plan" },
      { key: "result", label: "result", icon: "result" },
      { key: "history", label: "history", icon: "history" }
    ]
  }
];

const SETTINGS_NAV: NavEntry = { key: "settings", label: "settings", icon: "settings" };

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
      return worktrees.value.length > 0 ? `${worktrees.value.length}` : "";
    case "artifacts":
      return artifacts.value.length > 0 ? `${artifacts.value.length}` : "";
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
      return history.value.length > 0 ? `${history.value.length}` : "";
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
    <!-- `data-tauri-drag-region="deep"`: the window has no title bar on macOS, so
         the chrome that is left — this column and the header — is what a drag grabs,
         including its text and badges. Clickable children (buttons, links, fields)
         keep their own clicks, so the nav and the actions still work. -->
    <aside class="sidebar" data-tauri-drag-region="deep">
      <div class="brand">
        <img class="brand__mark" :src="brandLogo" alt="" />
        <div class="brand__text">
          <div class="brand__name">{{ messages.app.name }}</div>
          <div v-if="messages.app.tagline" class="brand__sub">{{ messages.app.tagline }}</div>
          <!-- The version belongs to the name, not to the fine print at the bottom:
               it identifies what this window is, and at the foot it read as one more
               note beside the read-only advisory. -->
          <div class="brand__ver">{{ messages.app.versionNote }}</div>
        </div>
      </div>

      <nav class="nav">
        <div
          v-for="section in NAV_SECTIONS"
          :key="section.label ?? 'home'"
          class="nav__group"
          :class="{ 'nav__group--home': section.label === null }"
        >
          <p v-if="section.label" class="nav__section">{{ messages.nav.groups[section.label] }}</p>
          <button
            v-for="item in section.items"
            :key="item.key"
            :data-view="item.key"
            class="nav__item"
            :class="view === item.key ? 'is-active' : ''"
            :aria-current="view === item.key ? 'page' : undefined"
            :disabled="!canOpen(item.key)"
            @click="goTo(item.key)"
          >
            <AppIcon :name="item.icon" class="nav__icon" />
            <span class="nav__label">{{ messages.nav[item.label] }}</span>
            <span v-if="navCount(item.key)" class="nav__count">{{ navCount(item.key) }}</span>
          </button>
        </div>
      </nav>

      <div class="sidebar__foot">
        <button
          :data-view="SETTINGS_NAV.key"
          class="nav__item"
          :class="view === SETTINGS_NAV.key ? 'is-active' : ''"
          :aria-current="view === SETTINGS_NAV.key ? 'page' : undefined"
          :disabled="!canOpen(SETTINGS_NAV.key)"
          @click="goTo(SETTINGS_NAV.key)"
        >
          <AppIcon :name="SETTINGS_NAV.icon" class="nav__icon" />
          <span class="nav__label">{{ messages.nav[SETTINGS_NAV.label] }}</span>
        </button>
        <div class="sidebar__notes">
          <p class="sidebar__note">{{ messages.app.readOnlyNote }}</p>
          <p v-if="!desktop" class="sidebar__note">{{ messages.app.previewNote }}</p>
        </div>
      </div>
    </aside>

    <main class="main">
      <header class="topbar" data-tauri-drag-region="deep">
        <!-- The path is shown whenever there is one, including *during* a scan:
             `projectPath` is set before the scan starts, and a bar that says "no
             project selected" while the app is visibly scanning that project
             reads as a bug. The counts and the actions wait for the result. -->
        <template v-if="projectPath">
          <span class="topbar__place">
            <AppIcon name="folder" class="topbar__icon" />
            <span class="topbar__path" :title="projectPath">{{ homePath(projectPath) }}</span>
          </span>
          <span v-if="scanState.running" class="faint tiny">{{ messages.scan.scanning }}</span>
          <span v-if="isDemoData" class="badge badge--accent">{{ messages.app.sampleData }}</span>
          <div v-if="hasSnapshot" class="topbar__group">
            <span class="badge badge--safe">{{ messages.status.short.Safe }} {{ totals.safe }}</span>
            <span class="badge badge--review">{{ messages.status.short.Review }} {{ totals.review }}</span>
            <span class="badge badge--unsafe">{{ messages.status.short.Unsafe }} {{ totals.unsafe }}</span>
          </div>
          <div v-if="hasSnapshot" class="topbar__actions">
            <button class="btn" @click="rescan">{{ messages.common.rescan }}</button>
            <button class="btn" @click="resetToWelcome">{{ messages.app.switchProject }}</button>
          </div>
        </template>
        <span v-else class="topbar__place">
          <AppIcon name="folder" class="topbar__icon" />
          <span class="topbar__path">{{ messages.app.noProject }}</span>
        </span>
        <!-- An update that is waiting is the one thing worth interrupting for, so
             it gets a button in the bar rather than a line on a page nobody opens.
             The button says which version is on offer and goes to Settings, where
             the download and the restart live. -->
        <button
          v-if="updateState.phase === 'available' || updateState.phase === 'ready'"
          class="btn btn--primary"
          data-update-available
          @click="goTo('settings')"
        >
          {{ messages.update.banner(updateState.version ?? "") }}
        </button>
      </header>

      <section class="content">
        <component :is="currentView" />
      </section>
    </main>
  </div>
</template>
