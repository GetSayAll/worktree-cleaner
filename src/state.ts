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
import { messages } from "./i18n";
import { pathMatchesRule } from "./format";
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
  welcome: messages.nav.project,
  scanning: messages.nav.scan,
  overview: messages.nav.overview,
  worktrees: messages.nav.worktrees,
  artifacts: messages.nav.artifacts,
  plan: messages.nav.plan,
  result: messages.nav.result,
  history: messages.nav.history,
  settings: messages.nav.settings
};

export const view = ref<ViewKey>("welcome");
export const projectPath = ref("");
export const snapshot = ref<ProjectSnapshot | null>(null);
export const settings = reactive<Settings>({ ...defaultSettings });
export const selectedWorktrees = ref<string[]>([]);
export const selectedArtifacts = ref<string[]>([]);
export const history = ref<HistoryEntry[]>([...mockHistory]);
export const lastResult = ref<ResultEntry[]>([]);
export const resultSummary = ref({ freed: 0, freed_unknown: 0, trashed: 0, removed: 0, failed: 0 });
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

export function applyDefaultSelection() {
  const snap = snapshot.value;
  if (!snap) return;
  selectedWorktrees.value = snap.worktrees.filter((w) => w.safety.status === "Safe").map((w) => w.path);
  selectedArtifacts.value = snap.artifacts
    .filter((a) => a.safety.status === "Safe")
    // An unmeasurable size is not "below the threshold": hiding it would silently
    // drop an item the user cannot assess (and `visibleArtifacts` keeps it for the
    // same reason).
    .filter((a) => a.size_bytes === null || a.size_bytes >= settings.min_artifact_mb * 1024 * 1024)
    .map((a) => a.path);
  // Artifacts inside a worktree that is also selected are removed with it; drop
  // them here so `selectedSummary` and `cleanupPlan` never disagree.
  pruneBundledArtifacts();
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
  artifacts.value.filter(
    (a) =>
      a.size_bytes === null ||
      a.size_bytes >= settings.min_artifact_mb * 1024 * 1024 ||
      a.safety.status === "Unsafe"
  )
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
  if (!snap) return { worktrees: 0, artifacts: 0, bytes: 0, unknown: 0 };
  const selectedWts = snap.worktrees.filter((w) => selectedWorktrees.value.includes(w.path));
  const wtBytes = selectedWts.reduce((sum, w) => sum + (w.size_bytes ?? 0), 0);
  const wtUnknown = selectedWts.filter((w) => w.size_bytes === null).length;
  // Artifacts removed together with a selected worktree are not counted again:
  // their bytes are already inside the worktree's size.
  const independentArtifacts = snap.artifacts.filter(
    (a) => selectedArtifacts.value.includes(a.path) && bundledOwner(a) === null
  );
  const afBytes = independentArtifacts.reduce((sum, a) => sum + (a.size_bytes ?? 0), 0);
  const afUnknown = independentArtifacts.filter((a) => a.size_bytes === null).length;
  return {
    worktrees: selectedWorktrees.value.length,
    artifacts: independentArtifacts.length,
    bytes: wtBytes + afBytes,
    unknown: wtUnknown + afUnknown
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
    total_bytes_unknown: 0,
    blockers: [],
    ack_reasons: []
  };
  if (!snap) return empty;

  const removalPaths = selectedWorktrees.value;

  const selectedWorktreeRemovals: WorktreeRemovalItem[] = snap.worktrees
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

  const selectedTrashMoves: TrashMoveItem[] = snap.artifacts
    .filter((a) => selectedArtifacts.value.includes(a.path))
    .map((a) => ({
      kind: "artifact" as const,
      path: a.path,
      artifact_kind: a.kind,
      size_bytes: a.size_bytes,
      file_count: a.file_count,
      bundled_with_worktree: bundledOwner(a)
    }));

  // A whitelist rule protects an item completely: it leaves both executable
  // channels and stays only in `blockers`. Otherwise the exported plan would name
  // the same path in `trash_moves` and in `blockers`, and the CLI could not accept
  // the plan unchanged.
  const isBlocked = (path: string) => settings.whitelist_paths.some((rule) => pathMatchesRule(path, rule));
  const blockers: string[] = [...selectedWorktreeRemovals, ...selectedTrashMoves]
    .filter((item) => isBlocked(item.path))
    .map((item) => messages.plan.blocker(item.path));

  const worktree_removals = selectedWorktreeRemovals.filter((w) => !isBlocked(w.path));
  const trash_moves = selectedTrashMoves.filter((t) => !isBlocked(t.path));

  const excluded_unsafe =
    snap.worktrees.filter((w) => w.safety.status === "Unsafe").length +
    snap.artifacts.filter((a) => a.safety.status === "Unsafe").length;

  // Only items deleted on their own are counted: an artifact marked
  // `bundled_with_worktree` disappears together with its worktree and is not a
  // separate deletion.
  const independentMoves = trash_moves.filter((t) => t.bundled_with_worktree === null);
  const total_bytes =
    worktree_removals.reduce((sum, w) => sum + (w.size_bytes ?? 0), 0) +
    independentMoves.reduce((sum, t) => sum + (t.size_bytes ?? 0), 0);
  // An unknown size contributes nothing to the total, so it must be counted
  // separately: the total is then a lower bound and the size threshold below
  // cannot be evaluated.
  const total_bytes_unknown =
    worktree_removals.filter((w) => w.size_bytes === null).length +
    independentMoves.filter((t) => t.size_bytes === null).length;

  const ack_reasons: string[] = [];
  const independentItems = worktree_removals.length + independentMoves.length;
  const forcedItems = worktree_removals.filter((w) => w.force);
  if (forcedItems.length > 0) {
    ack_reasons.push(messages.plan.ack.force(forcedItems.length));
  }
  if (independentItems >= 5) ack_reasons.push(messages.plan.ack.count(independentItems));
  if (total_bytes_unknown > 0) {
    // With unknown sizes in the plan the total is a lower bound, so the size
    // threshold below cannot be evaluated — this reason gates the run instead.
    ack_reasons.push(messages.plan.ack.unmeasurable(total_bytes_unknown));
  }
  if (total_bytes > 10 * 1024 * 1024 * 1024) {
    // The *known* part already exceeds the threshold, which is true regardless of
    // what the unmeasurable items turn out to be.
    ack_reasons.push(messages.plan.ack.size);
  }

  return {
    worktree_removals,
    trash_moves,
    excluded_unsafe,
    total_bytes,
    total_bytes_unknown,
    blockers,
    ack_reasons
  };
}

export function executeCleanup() {
  const plan = cleanupPlan.value;
  if (plan.worktree_removals.length === 0 && plan.trash_moves.length === 0 && plan.blockers.length === 0) return;
  const entries: ResultEntry[] = [];
  let freed = 0;
  let freedUnknown = 0;
  let trashed = 0;
  let removed = 0;
  let failed = 0;

  for (const item of plan.worktree_removals) {
    entries.push({
      action: item.command.startsWith("git worktree prune") ? "prune" : "worktree-remove",
      target: item.path,
      result: "ok",
      detail: item.command,
      // `null` stays `null`: an unmeasurable item must not be reported as 0 B
      // freed (development contract section 3).
      size_bytes: item.size_bytes
    });
    if (item.size_bytes === null) freedUnknown += 1;
    else freed += item.size_bytes;
    removed += 1;
  }

  for (const item of plan.trash_moves) {
    const bundled = item.bundled_with_worktree !== null;
    entries.push({
      action: "trash-move",
      target: item.path,
      result: "ok",
      detail: bundled ? messages.result.detail.bundled : messages.result.detail.trashed,
      size_bytes: bundled ? 0 : item.size_bytes
    });
    if (!bundled) {
      if (item.size_bytes === null) freedUnknown += 1;
      else freed += item.size_bytes;
      trashed += 1;
    }
  }

  if (plan.worktree_removals.length > 0) {
    entries.push({
      action: "prune",
      target: snapshot.value?.root ?? "",
      result: "ok",
      detail: messages.result.detail.pruneFinalize,
      size_bytes: 0
    });
  }

  for (const blocker of plan.blockers) {
    entries.push({
      action: "trash-move",
      target: blocker.replace(messages.plan.blockerPrefix, ""),
      result: "skipped",
      detail: messages.result.detail.preconditionFailed,
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
        ? messages.history.reason.confirmed
        : plan.trash_moves.find((t) => t.path === entry.target)
          ? messages.history.reason.rebuildable
          : messages.history.reason.metadata,
    result: entry.result,
    size_bytes: entry.size_bytes,
    detail: entry.detail
  }));
  history.value = [...newHistory, ...history.value];

  lastResult.value = entries;
  resultSummary.value = { freed, freed_unknown: freedUnknown, trashed, removed, failed };
  acknowledged.value = false;
  confirmText.value = "";
  view.value = "result";
}

export function sortReasons(reasons: Reason[]): Reason[] {
  return [...reasons].sort((a, b) => SEVERITY_RANK[b.severity] - SEVERITY_RANK[a.severity]);
}

export function primaryReason(reasons: Reason[]): Reason | null {
  const sorted = sortReasons(reasons);
  return sorted[0] ?? null;
}

watch(
  () => settings.theme,
  (theme) => {
    // Guarded so the module can be imported outside a DOM (for example by tests).
    if (typeof document === "undefined") return;
    const root = document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.dataset.theme = theme;
  },
  { immediate: true }
);
