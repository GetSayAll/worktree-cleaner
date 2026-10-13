import { computed, reactive, ref, watch } from "vue";
import type {
  ArtifactRecord,
  CleanupPlan,
  HistoryEntry,
  ProjectSnapshot,
  Reason,
  ResultEntry,
  Settings,
  TrashMoveItem,
  WorktreeRecord,
  WorktreeRemovalItem
} from "./types";
import { SEVERITY_RANK } from "./types";
import {
  defaultSettings,
  mockHistory,
  mockRoot,
  mockSnapshot,
  scanStages,
  scanTrace
} from "./mock/snapshot";

export type ViewKey =
  | "welcome"
  | "scanning"
  | "overview"
  | "worktrees"
  | "artifacts"
  | "plan"
  | "result"
  | "history"
  | "settings";

export const VIEW_LABEL: Record<ViewKey, string> = {
  welcome: "项目",
  scanning: "扫描",
  overview: "概览",
  worktrees: "Worktree",
  artifacts: "构建产物",
  plan: "清理计划",
  result: "执行结果",
  history: "历史",
  settings: "设置"
};

const WORKTREE_VIEWS: ViewKey[] = ["worktrees", "artifacts", "plan", "result", "history", "settings"];

export const view = ref<ViewKey>("welcome");
export const projectPath = ref("");
export const snapshot = ref<ProjectSnapshot | null>(null);
export const settings = reactive<Settings>({ ...defaultSettings });
export const selectedWorktrees = ref<string[]>([]);
export const selectedArtifacts = ref<string[]>([]);
export const history = ref<HistoryEntry[]>([...mockHistory]);
export const lastResult = ref<ResultEntry[]>([]);
export const resultSummary = ref({ freed: 0, trashed: 0, removed: 0, failed: 0 });
export const acknowledged = ref(false);
export const confirmText = ref("");

export const scanState = reactive({
  stageIndex: 0,
  traceLine: 0,
  elapsed: 0,
  cancelled: false,
  running: false
});

export const isDemoData = ref(false);

let stageTimer: number | null = null;
let traceTimer: number | null = null;
let clockTimer: number | null = null;

function clearTimers() {
  if (stageTimer !== null) window.clearInterval(stageTimer);
  if (traceTimer !== null) window.clearInterval(traceTimer);
  if (clockTimer !== null) window.clearInterval(clockTimer);
  stageTimer = null;
  traceTimer = null;
  clockTimer = null;
}

export function startScan(path: string) {
  const target = path.trim() || mockRoot;
  projectPath.value = target;
  selectedWorktrees.value = [];
  selectedArtifacts.value = [];
  lastResult.value = [];
  acknowledged.value = false;
  confirmText.value = "";
  scanState.stageIndex = 0;
  scanState.traceLine = 0;
  scanState.elapsed = 0;
  scanState.cancelled = false;
  scanState.running = true;
  view.value = "scanning";
  clearTimers();

  stageTimer = window.setInterval(() => {
    if (scanState.stageIndex < scanStages.length - 1) {
      scanState.stageIndex += 1;
    } else {
      finishScan();
    }
  }, 1100);

  traceTimer = window.setInterval(() => {
    if (scanState.traceLine < scanTrace.length - 1) scanState.traceLine += 1;
  }, 520);

  clockTimer = window.setInterval(() => {
    scanState.elapsed += 0.1;
  }, 100);
}

function finishScan() {
  clearTimers();
  scanState.running = false;
  scanState.stageIndex = scanStages.length - 1;
  snapshot.value = mockSnapshot;
  isDemoData.value = projectPath.value !== mockRoot;
  applyDefaultSelection();
  view.value = "overview";
}

export function cancelScan() {
  clearTimers();
  scanState.running = false;
  scanState.cancelled = true;
  view.value = "welcome";
}

export function resetToWelcome() {
  clearTimers();
  scanState.running = false;
  snapshot.value = null;
  projectPath.value = "";
  selectedWorktrees.value = [];
  selectedArtifacts.value = [];
  lastResult.value = [];
  view.value = "welcome";
}

export function goTo(next: ViewKey) {
  if (next === "scanning") return;
  view.value = next;
}

export function canOpen(key: ViewKey): boolean {
  if (key === "welcome" || key === "settings") return true;
  if (key === "result") return lastResult.value.length > 0;
  return snapshot.value !== null;
}

function applyDefaultSelection() {
  const snap = snapshot.value;
  if (!snap) return;
  selectedWorktrees.value = snap.worktrees.filter((w) => w.safety.status === "Safe").map((w) => w.path);
  selectedArtifacts.value = snap.artifacts
    .filter((a) => a.safety.status === "Safe")
    .filter((a) => (a.size_bytes ?? 0) >= settings.min_artifact_mb * 1024 * 1024)
    .map((a) => a.path);
}

export function isSelectedWorktree(path: string): boolean {
  return selectedWorktrees.value.includes(path);
}

export function isSelectedArtifact(path: string): boolean {
  return selectedArtifacts.value.includes(path);
}

export function toggleWorktree(path: string) {
  const record = snapshot.value?.worktrees.find((w) => w.path === path);
  if (!record || record.safety.status === "Unsafe") return;
  selectedWorktrees.value = isSelectedWorktree(path)
    ? selectedWorktrees.value.filter((p) => p !== path)
    : [...selectedWorktrees.value, path];
  pruneBundledArtifacts();
}

export function toggleArtifact(path: string) {
  const record = snapshot.value?.artifacts.find((a) => a.path === path);
  if (!record || record.safety.status === "Unsafe") return;
  if (bundledOwner(record) !== null) return;
  selectedArtifacts.value = isSelectedArtifact(path)
    ? selectedArtifacts.value.filter((p) => p !== path)
    : [...selectedArtifacts.value, path];
}

export function toggleGroupWorktrees(paths: string[], select: boolean) {
  const allowed = paths.filter((p) => {
    const record = snapshot.value?.worktrees.find((w) => w.path === p);
    return record && record.safety.status !== "Unsafe";
  });
  const set = new Set(selectedWorktrees.value);
  for (const p of allowed) {
    if (select) set.add(p);
    else set.delete(p);
  }
  selectedWorktrees.value = [...set];
  pruneBundledArtifacts();
}

export function toggleGroupArtifacts(paths: string[], select: boolean) {
  const allowed = paths.filter((p) => {
    const record = snapshot.value?.artifacts.find((a) => a.path === p);
    return record && record.safety.status !== "Unsafe" && bundledOwner(record) === null;
  });
  const set = new Set(selectedArtifacts.value);
  for (const p of allowed) {
    if (select) set.add(p);
    else set.delete(p);
  }
  selectedArtifacts.value = [...set];
}

function pruneBundledArtifacts() {
  const snap = snapshot.value;
  if (!snap) return;
  selectedArtifacts.value = selectedArtifacts.value.filter((path) => {
    const record = snap.artifacts.find((a) => a.path === path);
    if (!record) return false;
    const owner = bundledOwner(record);
    return owner === null || !selectedWorktrees.value.includes(owner);
  });
}

export function bundledOwner(record: ArtifactRecord): string | null {
  if (!record.worktree_path) return null;
  return selectedWorktrees.value.includes(record.worktree_path) ? record.worktree_path : null;
}

export function statusCount(records: Array<WorktreeRecord | ArtifactRecord>, status: string): number {
  return records.filter((r) => r.safety.status === status).length;
}

export const worktrees = computed<WorktreeRecord[]>(() => snapshot.value?.worktrees ?? []);
export const artifacts = computed<ArtifactRecord[]>(() => snapshot.value?.artifacts ?? []);

export const visibleArtifacts = computed<ArtifactRecord[]>(() =>
  artifacts.value.filter((a) => (a.size_bytes ?? 0) >= settings.min_artifact_mb * 1024 * 1024 || a.safety.status === "Unsafe")
);

export const counts = computed(() => ({
  worktree: {
    Safe: worktrees.value.filter((w) => w.safety.status === "Safe").length,
    Review: worktrees.value.filter((w) => w.safety.status === "Review").length,
    Unsafe: worktrees.value.filter((w) => w.safety.status === "Unsafe").length
  },
  artifact: {
    Safe: artifacts.value.filter((a) => a.safety.status === "Safe").length,
    Review: artifacts.value.filter((a) => a.safety.status === "Review").length,
    Unsafe: artifacts.value.filter((a) => a.safety.status === "Unsafe").length
  }
}));

export const selectedSummary = computed(() => {
  const snap = snapshot.value;
  if (!snap) return { worktrees: 0, artifacts: 0, bytes: 0 };
  const wtBytes = snap.worktrees
    .filter((w) => selectedWorktrees.value.includes(w.path))
    .reduce((sum, w) => sum + w.size_bytes, 0);
  const afBytes = snap.artifacts
    .filter((a) => selectedArtifacts.value.includes(a.path))
    .reduce((sum, a) => sum + (a.size_bytes ?? 0), 0);
  return {
    worktrees: selectedWorktrees.value.length,
    artifacts: selectedArtifacts.value.length,
    bytes: wtBytes + afBytes
  };
});

export const cleanupPlan = computed<CleanupPlan>(() => buildPlan());

function buildPlan(): CleanupPlan {
  const snap = snapshot.value;
  const empty: CleanupPlan = {
    worktree_removals: [],
    trash_moves: [],
    excluded_unsafe: 0,
    total_bytes: 0,
    blockers: [],
    ack_reasons: []
  };
  if (!snap) return empty;

  const removalPaths = selectedWorktrees.value;

  const worktree_removals: WorktreeRemovalItem[] = snap.worktrees
    .filter((w) => removalPaths.includes(w.path))
    .map((w) => {
      const needsForce = !w.is_prunable && w.untracked_files > 0;
      return {
        kind: "worktree" as const,
        path: w.path,
        branch: w.branch,
        command: w.is_prunable
          ? "git worktree prune"
          : `git worktree remove${needsForce ? " --force" : ""} "${w.path}"`,
        force: needsForce,
        untracked_files: w.untracked_file_list,
        size_bytes: w.size_bytes
      };
    });

  const trash_moves: TrashMoveItem[] = snap.artifacts
    .filter((a) => selectedArtifacts.value.includes(a.path))
    .map((a) => ({
      kind: "artifact" as const,
      path: a.path,
      artifact_kind: a.kind,
      size_bytes: a.size_bytes,
      file_count: a.file_count,
      bundled_with_worktree: bundledOwner(a)
    }));

  const excluded_unsafe =
    snap.worktrees.filter((w) => w.safety.status === "Unsafe").length +
    snap.artifacts.filter((a) => a.safety.status === "Unsafe").length;

  const total_bytes =
    worktree_removals.reduce((sum, w) => sum + w.size_bytes, 0) +
    trash_moves.filter((t) => t.bundled_with_worktree === null).reduce((sum, t) => sum + (t.size_bytes ?? 0), 0);

  const blockers: string[] = [];
  for (const item of [...worktree_removals.map((w) => w.path), ...trash_moves.map((t) => t.path)]) {
    const home = item.replace(/^\/Users\/[^/]+/, "~");
    for (const rule of settings.whitelist_paths) {
      if (home === rule || home.startsWith(`${rule}/`)) {
        blockers.push(`命中白名单保护：${item}`);
      }
    }
  }

  const ack_reasons: string[] = [];
  const totalItems = worktree_removals.length + trash_moves.length;
  const forcedItems = worktree_removals.filter((w) => w.force);
  if (forcedItems.length > 0) {
    ack_reasons.push(`包含 ${forcedItems.length} 个需要 --force 的 worktree，将一并删除其中的未跟踪文件`);
  }
  if (totalItems > 5) ack_reasons.push(`本次将处理 ${totalItems} 项，超过 5 项的确认阈值`);
  if (total_bytes > 10 * 1024 * 1024 * 1024) {
    ack_reasons.push("单次释放空间超过 10 GB");
  }

  return { worktree_removals, trash_moves, excluded_unsafe, total_bytes, blockers, ack_reasons };
}

export function executeCleanup() {
  const plan = cleanupPlan.value;
  const entries: ResultEntry[] = [];
  let freed = 0;
  let trashed = 0;
  let removed = 0;
  let failed = 0;

  for (const item of plan.worktree_removals) {
    entries.push({
      action: item.command.startsWith("git worktree prune") ? "prune" : "worktree-remove",
      target: item.path,
      result: "ok",
      detail: item.command,
      size_bytes: item.size_bytes
    });
    freed += item.size_bytes;
    removed += 1;
  }

  for (const item of plan.trash_moves) {
    const bundled = item.bundled_with_worktree !== null;
    entries.push({
      action: "trash-move",
      target: item.path,
      result: "ok",
      detail: bundled ? "随所属 worktree 一并移除，未单独操作" : "移入系统废纸篓，可在废纸篓中找回",
      size_bytes: bundled ? 0 : item.size_bytes ?? 0
    });
    if (!bundled) {
      freed += item.size_bytes ?? 0;
      trashed += 1;
    }
  }

  if (plan.worktree_removals.length > 0) {
    entries.push({
      action: "prune",
      target: snapshot.value?.root ?? "",
      result: "ok",
      detail: "git worktree prune 收尾，清理残留元数据",
      size_bytes: 0
    });
  }

  for (const blocker of plan.blockers) {
    entries.push({
      action: "trash-move",
      target: blocker.replace("命中白名单保护：", ""),
      result: "skipped",
      detail: "先决条件校验未通过，未执行",
      size_bytes: 0
    });
    failed += 1;
  }

  const now = new Date().toISOString();
  const newHistory: HistoryEntry[] = entries.map((entry) => ({
    at: now,
    action: entry.action,
    target: entry.target,
    reason:
      plan.worktree_removals.find((w) => w.path === entry.target)?.branch
        ? "用户确认后执行"
        : plan.trash_moves.find((t) => t.path === entry.target)
          ? "命中可重建产物类型"
          : "元数据清理",
    result: entry.result,
    size_bytes: entry.size_bytes,
    detail: entry.detail
  }));
  history.value = [...newHistory, ...history.value];

  lastResult.value = entries;
  resultSummary.value = { freed, trashed, removed, failed };
  acknowledged.value = false;
  confirmText.value = "";
  view.value = "result";
}

export function reasonClass(reason: Reason): string {
  return `reason-${reason.severity.toLowerCase()}`;
}

export function sortReasons(reasons: Reason[]): Reason[] {
  return [...reasons].sort((a, b) => SEVERITY_RANK[b.severity] - SEVERITY_RANK[a.severity]);
}

export function primaryReason(reasons: Reason[]): Reason | null {
  const sorted = sortReasons(reasons);
  return sorted[0] ?? null;
}

export function worktreeById(path: string | null): WorktreeRecord | null {
  if (!path) return null;
  return worktrees.value.find((w) => w.path === path) ?? null;
}

watch(
  () => settings.theme,
  (theme) => {
    const root = document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.dataset.theme = theme;
  },
  { immediate: true }
);

export { WORKTREE_VIEWS };
