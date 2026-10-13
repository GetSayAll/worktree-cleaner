import { computed, reactive, ref, watch } from "vue";
import type {
  ArtifactRecord,
  CleanupPlan,
  ExecutionOutcome,
  HistoryEntry,
  ProjectSnapshot,
  Reason,
  ResultEntry,
  ScanOptions,
  Settings,
  TrashMoveItem,
  WorktreeRecord,
  WorktreeRemovalItem
} from "./types";
import { SEVERITY_RANK } from "./types";
import { applyLocale, messages, watchSystemLocale } from "./i18n";
import { pathMatchesRule } from "./format";
import { executePlan, isDesktop, logPath, readHistory, scanProject } from "./platform";
import { mockHistory, mockSnapshot, scanTrace } from "./mock/snapshot";

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

/**
 * What one scan checks, in order.
 *
 * Keys rather than copy: the language can change at any time, so the stage
 * labels are looked up while rendering (see ScanningView).
 */
export const SCAN_STAGES = ["worktrees", "status", "artifacts", "verify"] as const;
export type ScanStage = (typeof SCAN_STAGES)[number];

/**
 * The settings a fresh install starts with.
 *
 * Deliberately empty: a whitelist rule or a recent project the user never chose
 * would look like their own configuration, and the scanner would act on it.
 */
const defaultSettings: Settings = {
  active_days: 7,
  min_artifact_mb: 10,
  untracked_is_unsafe: false,
  max_depth: 6,
  per_dir_timeout_sec: 30,
  scan_budget_sec: 600,
  extra_scan_paths: [],
  whitelist_paths: [],
  protected_branches: [],
  recent_projects: [],
  theme: "system",
  locale: "system",
  require_project_name: false
};

/** A project the user has scanned before. */
export interface RecentProject {
  path: string;
  scanned_at: string;
}

export const view = ref<ViewKey>("welcome");
export const projectPath = ref("");
export const snapshot = ref<ProjectSnapshot | null>(null);
export const settings = reactive<Settings>({ ...defaultSettings });
export const selectedWorktrees = ref<string[]>([]);
export const selectedArtifacts = ref<string[]>([]);
export const history = ref<HistoryEntry[]>([]);
export const historyError = ref<string | null>(null);
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

/** Why the last scan or execution did not run; `null` when it did. */
export const scanError = ref<string | null>(null);
export const executeError = ref<string | null>(null);
/** Set when the executor could not write the audit log: the actions still ran. */
export const logError = ref<string | null>(null);
export const executing = ref(false);

/**
 * Whether the pages are showing the sample snapshot.
 *
 * True in a browser preview, where there is no repository to read. The desktop
 * app only ever sets it to false: what it shows came from the real scanner.
 */
export const isDemoData = ref(false);

/** Projects scanned before, newest first. */
export const recentProjects = ref<RecentProject[]>([]);

const SETTINGS_KEY = "worktree-cleaner.settings";
const RECENTS_KEY = "worktree-cleaner.recents";
const RECENTS_LIMIT = 6;

// ---- persistence ------------------------------------------------------------

function storage(): Storage | null {
  // Guarded so the module can be imported outside a browser (for example by the
  // test runner): there is nothing to persist to there.
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

function loadSettings() {
  const store = storage();
  if (!store) return;
  try {
    const raw = store.getItem(SETTINGS_KEY);
    if (!raw) return;
    const stored = JSON.parse(raw) as Partial<Settings>;
    for (const [key, value] of Object.entries(stored)) {
      if (key in settings && value !== undefined) {
        (settings as Record<string, unknown>)[key] = value;
      }
    }
  } catch {
    // A corrupt entry is not worth a broken start: the defaults stand.
  }
}

function loadRecents() {
  const store = storage();
  if (!store) return;
  try {
    const raw = store.getItem(RECENTS_KEY);
    if (!raw) return;
    const stored = JSON.parse(raw) as unknown;
    if (!Array.isArray(stored)) return;
    recentProjects.value = stored.filter(
      (item): item is RecentProject =>
        typeof item === "object" &&
        item !== null &&
        typeof (item as RecentProject).path === "string"
    );
  } catch {
    recentProjects.value = [];
  }
}

/** Records that `path` was scanned just now, and keeps the list short. */
function rememberProject(path: string) {
  const entry: RecentProject = { path, scanned_at: new Date().toISOString() };
  recentProjects.value = [entry, ...recentProjects.value.filter((item) => item.path !== path)].slice(
    0,
    RECENTS_LIMIT
  );
  settings.recent_projects = recentProjects.value.map((item) => item.path);
  const store = storage();
  if (store) store.setItem(RECENTS_KEY, JSON.stringify(recentProjects.value));
}

export function forgetRecentProjects() {
  recentProjects.value = [];
  settings.recent_projects = [];
  const store = storage();
  if (store) store.removeItem(RECENTS_KEY);
}

loadSettings();
loadRecents();
applyLocale(settings.locale);
// Follows the OS language while the choice is "system"; a pinned choice is left
// alone, because the user asked for it explicitly.
watchSystemLocale();

// The choice is what the user picked; the table is rebound from it.
watch(
  () => settings.locale,
  (choice) => applyLocale(choice)
);

watch(
  settings,
  (current) => {
    const store = storage();
    if (store) store.setItem(SETTINGS_KEY, JSON.stringify(current));
  },
  { deep: true }
);

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

function startClock() {
  clockTimer = window.setInterval(() => {
    scanState.elapsed += 0.1;
  }, 100);
}

/**
 * How long each stage is shown before the marker moves on.
 *
 * The desktop scan is one call that answers when it is done, so the front end
 * cannot know which stage the core is in. What it can do is show that the work is
 * moving: without this the page sat on a dead-looking list while only the clock
 * advanced, and a user reasonably read that as a hang. The steps are labelled as
 * an indication of work under way, not as a report of position, and the copy on
 * the page says so. The walk stops at the last stage, so the marker never runs
 * past what the scan is doing and then sits there claiming more.
 */
const STAGE_INTERVAL_MS = 700;

/** Advances the step marker while a real scan runs. */
function startStageTimer() {
  stageTimer = window.setInterval(() => {
    if (scanState.stageIndex < SCAN_STAGES.length - 1) {
      scanState.stageIndex += 1;
    }
  }, STAGE_INTERVAL_MS);
}

/**
 * Budget for one Git command.
 *
 * Not a setting: a single hung `git` must not be able to stall a scan or an
 * execution, and the whole run is bounded separately by the scan budget.
 */
const GIT_TIMEOUT_MS = 10_000;

/** The scan knobs the core expects, derived from the settings page. */
function scanKnobs(): ScanOptions {
  return {
    max_depth: settings.max_depth,
    per_dir_timeout_ms: Math.max(1, settings.per_dir_timeout_sec * 1000),
    scan_budget_ms: Math.max(1, settings.scan_budget_sec * 1000),
    recent_activity_days: settings.active_days,
    untracked_as_unsafe: settings.untracked_is_unsafe,
    whitelist: settings.whitelist_paths,
    git_timeout_ms: GIT_TIMEOUT_MS
  };
}

/**
 * Scans `path`.
 *
 * The desktop app asks the core and shows what it answered. The browser preview
 * has no repository to read, so it walks the sample snapshot instead and every
 * page says the data is a sample.
 */
export async function startScan(path: string) {
  const target = path.trim();
  if (!target) return;

  projectPath.value = target;
  selectedWorktrees.value = [];
  selectedArtifacts.value = [];
  lastResult.value = [];
  acknowledged.value = false;
  confirmText.value = "";
  scanState.stageIndex = 0;
  scanState.traceLine = 0;  scanState.elapsed = 0;
  scanState.cancelled = false;
  scanState.running = true;
  scanError.value = null;
  executeError.value = null;
  logError.value = null;
  view.value = "scanning";
  clearTimers();
  startClock();

  if (!isDesktop()) {
    // The preview drives the steps itself and finishes on the last one, so it
    // takes the stage timer over rather than racing a second one for the handle.
    runPreviewScan();
    return;
  }
  startStageTimer();

  try {
    const result = await scanProject(target, scanKnobs());
    if (scanState.cancelled) return;
    finishScan(result);
  } catch (error) {
    if (scanState.cancelled) return;
    scanState.running = false;
    clearTimers();
    scanError.value = error instanceof Error ? error.message : String(error);
    view.value = "welcome";
  }
}

/** The preview's stand-in for a scan: it reveals the sample snapshot in steps. */
function runPreviewScan() {
  stageTimer = window.setInterval(() => {
    if (scanState.stageIndex < SCAN_STAGES.length - 1) {
      scanState.stageIndex += 1;
    } else {
      finishScan(mockSnapshot, true);
    }
  }, 1100);

  traceTimer = window.setInterval(() => {
    if (scanState.traceLine < scanTrace.length - 1) scanState.traceLine += 1;
  }, 520);
}

function finishScan(result: ProjectSnapshot, sample = false) {
  clearTimers();
  scanState.running = false;
  scanState.stageIndex = SCAN_STAGES.length - 1;
  snapshot.value = result;
  isDemoData.value = sample;
  rememberProject(projectPath.value);
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
  scanError.value = null;
  executeError.value = null;
  logError.value = null;
  view.value = "welcome";
}

export function goTo(next: ViewKey) {
  if (next === "scanning") return;
  if (next === "history") void refreshHistory();
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

/**
 * The selected paths a whitelist rule keeps out of the plan.
 *
 * Computed once and used both for the blocker text and for filtering the two
 * channels: deriving them separately is exactly how the plan the user exports
 * and the plan the executor sees would end up naming different items.
 */
export const blockedPaths = computed<string[]>(() => {
  const snap = snapshot.value;
  if (!snap) return [];
  const selected = [
    ...snap.worktrees.filter((w) => selectedWorktrees.value.includes(w.path)).map((w) => w.path),
    ...snap.artifacts.filter((a) => selectedArtifacts.value.includes(a.path)).map((a) => a.path)
  ];
  return selected.filter((path) =>
    settings.whitelist_paths.some((rule) => pathMatchesRule(path, rule))
  );
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
          // `--` before the path, as the executor does: what is shown has to be what
          // runs.
          : `git worktree remove${needsForce ? " --force" : ""} -- "${w.path}"`,
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
  const blocked = new Set(blockedPaths.value);
  const blockers: string[] = [
    ...selectedWorktreeRemovals,
    ...selectedTrashMoves
  ]
    .filter((item) => blocked.has(item.path))
    .map((item) => messages.plan.blocker(item.path));

  const worktree_removals = selectedWorktreeRemovals.filter((w) => !blocked.has(w.path));
  const trash_moves = selectedTrashMoves.filter((t) => !blocked.has(t.path));

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

/**
 * Runs the confirmed plan.
 *
 * Nothing is simulated here: the core re-checks every item, executes what still
 * holds, and answers item by item. What it could not do comes back with its
 * reason rather than being smoothed over.
 */
export async function executeCleanup() {
  const plan = cleanupPlan.value;
  const current = snapshot.value;
  if (!current) return;
  if (
    plan.worktree_removals.length === 0 &&
    plan.trash_moves.length === 0 &&
    plan.blockers.length === 0
  ) {
    return;
  }

  if (!isDesktop()) {
    // The preview cannot reach the file system, and there is no honest way to
    // pretend otherwise: it says why instead of showing a fake result page.
    executeError.value = messages.app.desktopOnly;
    return;
  }

  executing.value = true;
  executeError.value = null;
  logError.value = null;
  try {
    const outcome = await executePlan(plan, current, {
      recent_activity_days: settings.active_days,
      untracked_as_unsafe: settings.untracked_is_unsafe,
      whitelist: settings.whitelist_paths,
      // The same two budgets the scan used. The re-check measures the very same
      // directories, so a setting that shaped the scan and was then replaced by a
      // default here would mean one thing on the way in and another on the way out.
      per_dir_timeout_ms: Math.max(1, settings.per_dir_timeout_sec * 1000),
      git_timeout_ms: GIT_TIMEOUT_MS,
      // `null` disables the audit log, which the user is then told about.
      log_path: await logPath()
    });
    await applyOutcome(outcome, plan);
  } catch (error) {
    executeError.value = error instanceof Error ? error.message : String(error);
  } finally {
    executing.value = false;
  }
}

/** Turns the executor's answer into what the result page shows. */
async function applyOutcome(outcome: ExecutionOutcome, plan: CleanupPlan) {
  // A whitelist-blocked item never reaches the executor — it was filtered out of
  // both channels before the plan was built — so it is added back here as its own
  // skipped row. Leaving it out would make the plan page's count disagree with
  // the rows on this one.
  const blocked: ResultEntry[] = blockedPaths.value.map((path) => ({
    action: "trash-move",
    target: path,
    result: "skipped",
    detail: messages.result.detail.preconditionFailed,
    size_bytes: null
  }));

  lastResult.value = [...outcome.entries, ...blocked];
  resultSummary.value = summarise(lastResult.value, plan);
  logError.value = outcome.log_error;
  acknowledged.value = false;
  confirmText.value = "";
  view.value = "result";
  // Awaited so the history on screen is the log the run just wrote, not the one
  // that was there before it.
  await refreshHistory();
}

/**
 * Counts a run for the four cards.
 *
 * Only actions that really removed something contribute bytes: a `prune` entry
 * cleans metadata and frees nothing, and a skipped one removed nothing at all. A
 * `null` size counts as unmeasurable rather than as zero (section 3).
 *
 * An artifact that left together with its worktree is not a separate move. The
 * executor reports it as 0 bytes, which is indistinguishable from a genuinely empty
 * directory — so the plan, which knows which is which, decides.
 */
function summarise(entries: ResultEntry[], plan: CleanupPlan) {
  const bundled = new Set(
    plan.trash_moves
      .filter((move) => move.bundled_with_worktree !== null)
      .map((move) => move.path)
  );
  let freed = 0;
  let freedUnknown = 0;
  let trashed = 0;
  let removed = 0;
  let failed = 0;

  for (const entry of entries) {
    if (entry.result !== "ok") {
      failed += 1;
      continue;
    }
    switch (entry.action) {
      case "worktree-remove":
        removed += 1;
        break;
      case "trash-move":
        if (!bundled.has(entry.target)) trashed += 1;
        break;
      case "prune":
        continue;
    }
    if (entry.size_bytes === null) freedUnknown += 1;
    else freed += entry.size_bytes;
  }

  return { freed, freed_unknown: freedUnknown, trashed, removed, failed };
}

/**
 * Reloads the audit trail.
 *
 * The desktop app reads the log file it wrote; the preview shows the sample
 * records, because it has no log to read.
 */
export async function refreshHistory() {
  if (!isDesktop()) {
    history.value = [...mockHistory].reverse();
    historyError.value = null;
    return;
  }
  try {
    // The log is appended oldest first; every page shows the newest on top.
    history.value = (await readHistory()).reverse();
    historyError.value = null;
  } catch (error) {
    historyError.value = error instanceof Error ? error.message : String(error);
  }
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
