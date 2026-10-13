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
  { key: "welcome", label: "项目" },
  { key: "overview", label: "概览" },
  { key: "worktrees", label: "Worktree" },
  { key: "artifacts", label: "构建产物" },
  { key: "plan", label: "清理计划" },
  { key: "result", label: "执行结果" },
  { key: "history", label: "历史" },
  { key: "settings", label: "设置" }
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
        <div class="brand__name">Worktree Cleaner</div>
        <div class="brand__sub">安全清理审计</div>
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
        <p class="sidebar__note">扫描阶段只跑 Git 只读命令；删除统一走系统废纸篓，不永久删除。</p>
        <p class="sidebar__note">v0.1.0 · 界面原型</p>
      </div>
    </aside>

    <main class="main">
      <header class="topbar">
        <template v-if="hasSnapshot">
          <span class="topbar__path" :title="projectPath">{{ homePath(projectPath) }}</span>
          <span v-if="isDemoData" class="badge badge--accent">示例数据</span>
          <div class="topbar__group">
            <span class="badge badge--safe">可删 {{ totals.safe }}</span>
            <span class="badge badge--review">待确认 {{ totals.review }}</span>
            <span class="badge badge--unsafe">不可删 {{ totals.unsafe }}</span>
          </div>
          <button class="btn" @click="rescan">重新扫描</button>
          <button class="btn" @click="resetToWelcome">切换项目</button>
        </template>
        <template v-else>
          <span class="topbar__path">未选择项目</span>
          <span class="faint tiny">选择一个仓库开始扫描</span>
        </template>
      </header>

      <section class="content">
        <component :is="currentView" />
      </section>
    </main>
  </div>
</template>
