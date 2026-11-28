import { computed, reactive, ref, watch } from "vue";
import type {
  ArtifactRecord,
  CleanupPlan,
  ExecutionOutcome,
  HistoryEntry,
  CacheSnapshot,
  GeneratedCacheSnapshot,
  ProjectSnapshot,
  Reason,
  ResultEntry,
  ScanFailure,
  ScanOptions,
  Settings,
  TrashMoveItem,
  WorkspaceSnapshot,
  WorktreeRecord,
  WorktreeRemovalItem
} from "./types";
import { SEVERITY_RANK } from "./types";
import { applyLocale, messages, watchSystemLocale } from "./i18n";
import {
  compareSizeNullsLast,
  formatBytes,
  isUnder,
  normalizePathKey,
  pathIdentityKey,
  pathMatchesRule
} from "./format";
import {
  executePlan,
  hostOs,
  isDesktop,
  logActivity,
  logPath,
  onExecutionProgress,
  readActivity,
  readHistory,
  scanCaches,
  scanGeneratedCaches,
  scanProject,
  scanWorkspace,
  executeCachePlan,
  executeGeneratedCachePlan,
  retireClone
} from "./platform";
import {
  mockActivity,
  mockCacheSnapshot,
  mockGeneratedCacheSnapshot,
  mockHistory,
  mockSnapshot,
  mockWorkspaceSnapshot,
  scanTrace
} from "./mock/snapshot";
import type { ActivityAction, ActivityEvent, DetailArg } from "./types";

export type ViewKey =
  | "welcome"
  | "scanning"
  | "workspace"
  | "caches"
  | "buildCaches"
  | "overview"
  | "worktrees"
  | "artifacts"
  | "hygiene"
  | "plan"
  | "retire"
  | "result"
  | "history"
  | "settings";

/**
 * What one scan checks, in order.
 *
 * Keys rather than copy: the language can change at any time, so the stage
 * labels are looked up while rendering (see ScanningView).
 */
export const SCAN_STAGES = ["worktrees", "status", "artifacts", "cache", "verify"] as const;
export type ScanStage = (typeof SCAN_STAGES)[number];

/**
 * The settings a fresh install starts with.
 *
 * Deliberately empty: a whitelist rule or a recent project the user never chose
 * would look like their own configuration, and the scanner would act on it.
 */
const defaultSettings: Settings = {
  active_days: 3,
  min_artifact_mb: 10,
  untracked_is_unsafe: false,
  max_depth: 6,
  per_dir_timeout_sec: 300,
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
/** Repositories found below a directory that is not itself a Git repository. */
export const workspaceSnapshot = ref<WorkspaceSnapshot | null>(null);

export type WorkspaceScanStatus = "pending" | "scanning" | "done" | "failed";

/**
 * One repository's place in a workspace-wide scan.
 *
 * The snapshots stay separate on purpose: worktree verdicts are anchored to each
 * repository's default branch, and the cleanup plan is always for one
 * repository. This is a progress record, not a merged project.
 */
export interface WorkspaceScanEntry {
  status: WorkspaceScanStatus;
  snapshot: ProjectSnapshot | null;
  failure: ScanFailure | null;
}

/** Per-repository results for the workspace-wide scan, keyed by path. */
export const workspaceScanEntries = ref<Record<string, WorkspaceScanEntry>>({});

/** Progress of the workspace-wide scan; it stays on the repository page. */
export const workspaceScanState = reactive({
  running: false,
  completed: 0,
  total: 0,
  current: ""
});

/**
 * Totals across the repositories scanned in this workspace.
 *
 * This counts what is on screen and what each independent plan can see; it is
 * never used to build one cross-repository cleanup plan.
 */
export const workspaceScanSummary = computed(() => {
  const entries = Object.values(workspaceScanEntries.value);
  const snapshots = entries
    .filter(
      (entry): entry is WorkspaceScanEntry & { snapshot: ProjectSnapshot } =>
        entry.status === "done" && entry.snapshot !== null
    )
    .map((entry) => entry.snapshot);
  const counts = { Safe: 0, Review: 0, Unsafe: 0 };
  let worktrees = 0;
  let artifacts = 0;
  for (const snapshot of snapshots) {
    worktrees += snapshot.worktrees.length;
    artifacts += snapshot.artifacts.length;
    for (const record of [...snapshot.worktrees, ...snapshot.artifacts]) {
      counts[record.safety.status] += 1;
    }
  }
  return {
    done: snapshots.length,
    failed: entries.filter((entry) => entry.status === "failed").length,
    worktrees,
    artifacts,
    ...counts
  };
});

/**
 * The folder from the last repository discovery.
 *
 * A normal single-repository scan clears `workspaceSnapshot`; a cached batch
 * result keeps it so the repository list remains one click away. Either way this
 * folder stays available to the build-cache page, which looks below the folder the
 * reader scanned rather than inside one repository.
 */
export const lastWorkspaceRoot = ref("");
/** The cache inventory is a separate surface from the project snapshot. */
export const cacheSnapshot = ref<CacheSnapshot | null>(null);
export const selectedCaches = ref<string[]>([]);
export const cacheScanning = ref(false);
export const cacheError = ref(false);

/** Build caches are a third, folder-scoped surface: not a project and not a known system root. */
export const buildCacheRoot = ref("");
export const buildCacheSnapshot = ref<GeneratedCacheSnapshot | null>(null);
export const selectedBuildCaches = ref<string[]>([]);
export const buildCacheScanning = ref(false);
export const buildCacheError = ref(false);
export const buildCacheSample = ref(false);
export const buildCacheAcknowledged = ref(false);
export const buildCacheLastResult = ref<ResultEntry[]>([]);
export const buildCacheResultSummary = ref({
  freed: 0,
  freed_unknown: 0,
  trash_bytes: 0,
  trash_unknown: 0,
  reclaimable: 0,
  reclaimable_unknown: 0,
  trashed: 0,
  removed: 0,
  failed: 0
});
export const settings = reactive<Settings>({ ...defaultSettings });
export const selectedWorktrees = ref<string[]>([]);
export const selectedArtifacts = ref<string[]>([]);
export const history = ref<HistoryEntry[]>([]);
/**
 * What the user did, newest first.
 *
 * Kept beside `history` rather than folded into it: the two records answer
 * different questions, and the history page shows them as one timeline with the
 * kind marked, rather than pretending a setting change is a deletion.
 */
export const activity = ref<ActivityEvent[]>([]);
export const historyError = ref(false);
export const lastResult = ref<ResultEntry[]>([]);
export const resultSummary = ref({
  freed: 0,
  freed_unknown: 0,
  trash_bytes: 0,
  trash_unknown: 0,
  reclaimable: 0,
  reclaimable_unknown: 0,
  trashed: 0,
  removed: 0,
  failed: 0
});
/** Cache cleanup is a separate surface; its result must not replace a project run. */
export const cacheLastResult = ref<ResultEntry[]>([]);
export const cacheResultSummary = ref({
  freed: 0,
  freed_unknown: 0,
  trash_bytes: 0,
  trash_unknown: 0,
  reclaimable: 0,
  reclaimable_unknown: 0,
  trashed: 0,
  removed: 0,
  failed: 0
});
export const cacheAcknowledged = ref(false);
export const cacheSample = ref(false);
export const acknowledged = ref(false);
export const confirmText = ref("");

/**
 * What the last recorded plan held, so the same plan is not written twice.
 *
 * Module state, not a ref: nothing renders it. Reopening the plan page is not a
 * new plan; changing a selection is. The signature is the contents rather than a
 * count, because two different selections can hold the same number of items.
 */
let recordedPlan = "";

/** A scan that is superseded by a newer run must not finish the newer one. */
let scanRun = 0;
let buildCacheRun = 0;
/** The workspace-wide scan has its own generation, so cancelling one never revives it. */
let workspaceScanRun = 0;

export const scanState = reactive({
  stageIndex: 0,
  traceLine: 0,
  elapsed: 0,
  cancelled: false,
  running: false
});

/** Why the last scan did not run; `null` when it did. */
export const scanFailure = ref<ScanFailure | null>(null);
/** A stable execution failure code; the copy is looked up while rendering. */
export const executeError = ref<"desktop-only" | "failed" | null>(null);
export const executeErrorMessage = computed(() => {
  if (executeError.value === "desktop-only") return messages.app.desktopOnly;
  if (executeError.value === "failed") return messages.errors.execute;
  return null;
});
/** Set when the executor could not write the audit log: the actions still ran. */
export const logError = ref<string | null>(null);
export const executing = ref(false);
/** Results that have arrived while the execution is still running, in order. */
export const executionProgress = ref<ResultEntry[]>([]);

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
const SETTINGS_VERSION_KEY = "worktree-cleaner.settings-version";
const SETTINGS_VERSION = 1;
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
    migrateSettings(store, stored);
    for (const [key, value] of Object.entries(stored)) {
      if (key in settings && value !== undefined) {
        (settings as Record<string, unknown>)[key] = value;
      }
    }
  } catch {
    // A corrupt entry is not worth a broken start: the defaults stand.
  }
}

/**
 * Moves installs that still hold the old scan defaults onto the current ones.
 *
 * A stored settings object wins over a changed default. Existing installs still
 * held 7 days and 30 seconds, so changing the default alone would never reach
 * the scan that made recently written build output `Review`. Only the exact old
 * pair is migrated; a user who changed either value keeps their choice, and the
 * separate version key makes the migration one-time.
 */
function migrateSettings(store: Storage, stored: Partial<Settings>) {
  const version = Number(store.getItem(SETTINGS_VERSION_KEY) ?? "0");
  if (version >= SETTINGS_VERSION) return;
  if (stored.active_days === 7 && stored.per_dir_timeout_sec === 30) {
    stored.active_days = 3;
    stored.per_dir_timeout_sec = 300;
  }
  store.setItem(SETTINGS_VERSION_KEY, String(SETTINGS_VERSION));
}

function loadRecents() {
  const store = storage();
  if (!store) return;
  try {
    const raw = store.getItem(RECENTS_KEY);
    if (!raw) return;
    const stored = JSON.parse(raw) as unknown;
    if (!Array.isArray(stored)) return;
    const seen = new Set<string>();
    recentProjects.value = stored
      .filter(
        (item): item is RecentProject =>
          typeof item === "object" &&
          item !== null &&
          typeof (item as RecentProject).path === "string"
      )
      // One project, one row: an older build stored the same directory under two
      // separator or case spellings.
      .filter((item) => {
        const key = recentKey(item.path);
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      });
  } catch {
    recentProjects.value = [];
  }
}

/**
 * What two spellings of one project have in common.
 *
 * Windows accepts both separators and is case-insensitive, so `C:\src\repo` and
 * `c:/SRC/repo` are one directory. macOS is compared as written: its default
 * volume folds case, but a case-sensitive one is legal and there the difference is
 * real.
 */
function recentKey(path: string): string {
  return pathIdentityKey(path, hostOs() === "windows");
}

/** Records that `path` was scanned just now, and keeps the list short. */
function rememberProject(path: string) {
  const previous = projectPath.value;
  if (previous && previous !== path) {
    record("project.switched", path, { from: previous });
  }
  const entry: RecentProject = { path, scanned_at: new Date().toISOString() };
  const key = recentKey(path);
  recentProjects.value = [
    entry,
    ...recentProjects.value.filter((item) => recentKey(item.path) !== key)
  ].slice(0, RECENTS_LIMIT);
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

/** Drops one path after the action removed the directory it named. */
function forgetProject(path: string) {
  const key = recentKey(path);
  recentProjects.value = recentProjects.value.filter((item) => recentKey(item.path) !== key);
  settings.recent_projects = recentProjects.value.map((item) => item.path);
  const store = storage();
  if (store) store.setItem(RECENTS_KEY, JSON.stringify(recentProjects.value));
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
    // The last stage is "Summarize"; the real build-cache scan owns the
    // penultimate marker, and a timer that ran ahead would jump back when
    // enterBuildCacheStage parks it there.
    if (scanState.stageIndex < SCAN_STAGES.length - 2) {
      scanState.stageIndex += 1;
    }
  }, STAGE_INTERVAL_MS);
}

/**
 * Holds the scan page on the build-cache step while that scan is working.
 *
 * The ordinary stage timer is a liveness cue, not a real progress report; the
 * build-cache scan is a real awaited step after the repository scan, so the
 * marker is set here instead of being allowed to run on to "Summarize" while it
 * still runs.
 */
function enterBuildCacheStage() {
  if (stageTimer !== null) {
    window.clearInterval(stageTimer);
    stageTimer = null;
  }
  scanState.stageIndex = SCAN_STAGES.length - 2;
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
/**
 * Writes one line to the activity log.
 *
 * Fire and forget. The record describes what happened, and the only person who
 * needs it is one looking back at it later, so a failed write must not interrupt
 * a scan or a deletion — nothing here is awaited and nothing here throws.
 */
function record(action: ActivityAction, target = "", args: Record<string, DetailArg> = {}): void {
  void logActivity(action, target, args).catch(() => undefined);
}

/** Records a setting change with what it was, so a later reading can compare. */
export function recordSettingChange(key: string, before: string, after: string): void {
  if (before === after) return;
  record("settings.changed", key, { before, after });
}

/** Turns the Tauri rejection into the code the welcome page switches on. */
function normaliseScanFailure(error: unknown, path: string): ScanFailure {
  if (typeof error === "object" && error !== null && "code" in error) {
    const code = (error as { code?: unknown }).code;
    if (typeof code === "string") return error as ScanFailure;
  }
  // A shell that serializes a command error as a string still gets the useful
  // copy for the errors a reader can act on. No branch prints `detail`.
  const detail = error instanceof Error ? error.message : String(error);
  if (detail.includes("not a Git repository")) {
    return { code: "not_a_git_repository", path, detail };
  }
  if (detail.includes("does not exist")) {
    return { code: "not_found", path };
  }
  if (detail.includes("git could not be queried") || detail.includes("git command")) {
    return { code: "git_unavailable", detail };
  }
  return { code: "unknown", detail };
}

/**
 * The folder build-cache discovery should use for this scan.
 *
 * A repository opened from a workspace keeps that workspace as its cache scope:
 * `design-artifacts` or a shared build directory usually lives beside the
 * repository, not inside it. A direct repository scan has no such scope and
 * searches below the repository itself.
 */
function cacheScopeFor(workspaceRoot: string, requested?: string): string {
  const explicit = requested?.trim();
  if (explicit) return explicit;
  const previous = lastWorkspaceRoot.value;
  if (previous && isUnder(workspaceRoot, previous)) return previous;
  const recentAncestor = recentProjects.value
    .map((entry) => entry.path)
    .filter((path) => path !== workspaceRoot && isUnder(workspaceRoot, path))
    .sort((left, right) => left.length - right.length)[0];
  return recentAncestor ?? workspaceRoot;
}

/**
 * The project and a few parent folders to search for tagged build caches.
 *
 * A repository is normally opened from a developer root, while its build output
 * may sit beside it under that root (`design-artifacts`, a shared build cache).
 * The user selects the repository, not the cache folder; checking the root and
 * two parent folders finds that common layout without asking them to know the
 * path. The home directory and filesystem roots are never searched: a project
 * needs at least three path components for an ancestor to be considered.
 */
export function cacheSearchScopes(root: string): string[] {
  let current = normalizePathKey(root).replace(/\/+$/, "");
  const scopes: string[] = [];
  for (let level = 0; level < 3; level += 1) {
    const parts = current.split("/").filter(Boolean);
    // A Windows drive letter is part of the path but not a normal directory
    // name. Without subtracting it, `C:\Users\andy` looked like three
    // components and a project directly under the user's home searched the
    // whole home directory.
    const components = parts.length - (/^[A-Za-z]:$/.test(parts[0] ?? "") ? 1 : 0);
    if (components < 3) break;
    scopes.push(current);
    const slash = current.lastIndexOf("/");
    if (slash <= 0) break;
    current = current.slice(0, slash);
  }
  return scopes.length > 0 ? scopes : [root];
}

export async function startScan(
  path: string,
  buildCacheScope?: string,
  expandRelatedClones = true
) {
  if (executing.value) return;
  const target = path.trim();
  if (!target) return;

  // Opening one repository is a single-repository run; any unfinished batch
  // belongs to the previous view and must not keep writing into this one.
  stopWorkspaceScan();
  workspaceScanEntries.value = {};

  const runId = ++scanRun;
  // Stop a page-level build-cache refresh from writing into this run's state.
  buildCacheRun += 1;
  buildCacheScanning.value = false;
  record("scan.started", target);
  projectPath.value = target;
  selectedWorktrees.value = [];
  selectedArtifacts.value = [];
  lastResult.value = [];
  acknowledged.value = false;
  confirmText.value = "";
  // A new scan makes a new plan; the same selection is now a different run.
  recordedPlan = "";
  scanState.stageIndex = 0;
  scanState.traceLine = 0;  scanState.elapsed = 0;
  scanState.cancelled = false;
  scanState.running = true;
  scanFailure.value = null;
  executeError.value = null;
  logError.value = null;
  workspaceSnapshot.value = null;
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
    // A repository row opened from the workspace is already the chosen project.
    // Skipping discovery here prevents the automatic same-remote expansion from
    // turning that click back into the workspace page.
    if (!expandRelatedClones) {
      const cacheScope = buildCacheScope?.trim() || target;
      const result = await scanProject(target, scanKnobs());
      if (scanState.cancelled || runId !== scanRun) return;
      buildCacheRoot.value = cacheScope;
      enterBuildCacheStage();
      await runBuildCacheScan(cacheScope, false);
      if (scanState.cancelled || runId !== scanRun) return;
      finishScan(result);
      return;
    }

    // A developer root is often not a repository itself. Discovery is cheap and
    // read-only; it either confirms this is one repository, offers the nested
    // repositories, or produces an honest empty answer.
    const workspace = await scanWorkspace(target);
    if (scanState.cancelled || runId !== scanRun) return;
    if (workspace.repositories.length === 0) {
      scanState.running = false;
      clearTimers();
      scanFailure.value = { code: "no_repositories", path: target };
      record("scan.failed", target, { error: "no_repositories" });
      view.value = "welcome";
      return;
    }
    const explicitCacheScope = Boolean(buildCacheScope?.trim());
    const cacheScope =
      workspace.repositories.length > 1
        ? workspace.root
        : cacheScopeFor(workspace.root, buildCacheScope);
    if (workspace.repositories.length > 1) {
      buildCacheRoot.value = cacheScope;
      enterBuildCacheStage();
      await runBuildCacheScan(cacheScope, false);
      if (scanState.cancelled || runId !== scanRun) return;
      finishWorkspace(workspace);
      return;
    }

    const repository = workspace.repositories[0];
    projectPath.value = repository.path;
    const result = await scanProject(repository.path, scanKnobs());
    if (scanState.cancelled || runId !== scanRun) return;
    buildCacheRoot.value = cacheScope;
    enterBuildCacheStage();
    // A repository opened from a workspace has that workspace as its cache
    // scope. Only a direct repository scan widens the search to nearby parents.
    await runBuildCacheScan(cacheScope, !explicitCacheScope);
    if (scanState.cancelled || runId !== scanRun) return;
    finishScan(result);
  } catch (error) {
    if (scanState.cancelled || runId !== scanRun) return;
    scanState.running = false;
    clearTimers();
    const failure = normaliseScanFailure(error, target);
    scanFailure.value = failure;
    const detail =
      "detail" in failure ? failure.detail : "path" in failure ? failure.path : failure.code;
    record("scan.failed", target, { error: detail });
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

/**
 * Shows the repositories discovered below a workspace root.
 *
 * Discovery does not start a project scan: each row is a way into the existing
 * one-repository flow, which keeps every worktree verdict anchored to that
 * repository's default branch.
 */
function finishWorkspace(workspace: WorkspaceSnapshot) {
  clearTimers();
  scanState.running = false;
  scanState.stageIndex = SCAN_STAGES.length - 1;
  workspaceSnapshot.value = workspace;
  lastWorkspaceRoot.value = workspace.root;
  snapshot.value = null;
  isDemoData.value = false;
  rememberProject(workspace.root);
  record("workspace.discovered", workspace.root, {
    repositories: workspace.repositories.length
  });
  view.value = "workspace";
}

/** Stops a workspace-wide scan without publishing a partial result as complete. */
function stopWorkspaceScan() {
  workspaceScanRun += 1;
  workspaceScanState.running = false;
  workspaceScanState.current = "";
}

/** Cancels the workspace-wide scan and leaves the unfinished row unscanned. */
export function cancelWorkspaceScan() {
  if (!workspaceScanState.running) return;
  stopWorkspaceScan();
  workspaceScanEntries.value = Object.fromEntries(
    Object.entries(workspaceScanEntries.value).map(([path, entry]) => [
      path,
      entry.status === "scanning" ? { ...entry, status: "pending" as const } : entry
    ])
  );
}

/**
 * Scans every repository in the discovered workspace.
 *
 * The scans run one after another: each one is a real repository scan, and six
 * simultaneous walks would contend for the same disk and make the progress list
 * jump around without making the total faster. Each result stays a separate
 * snapshot, so opening one repository never blends its cleanup plan with another.
 */
export async function scanAllWorkspaceRepositories() {
  const workspace = workspaceSnapshot.value;
  if (
    !workspace ||
    workspace.repositories.length === 0 ||
    !isDesktop() ||
    executing.value ||
    scanState.running ||
    workspaceScanState.running
  ) {
    return;
  }

  const runId = ++workspaceScanRun;
  workspaceScanState.running = true;
  workspaceScanState.completed = 0;
  workspaceScanState.total = workspace.repositories.length;
  workspaceScanState.current = "";
  workspaceScanEntries.value = Object.fromEntries(
    workspace.repositories.map((repository) => [
      repository.path,
      { status: "pending" as const, snapshot: null, failure: null }
    ])
  );

  const setEntry = (path: string, entry: WorkspaceScanEntry) => {
    workspaceScanEntries.value = { ...workspaceScanEntries.value, [path]: entry };
  };

  try {
    for (const [index, repository] of workspace.repositories.entries()) {
      if (runId !== workspaceScanRun) return;
      workspaceScanState.current = repository.path;
      setEntry(repository.path, { status: "scanning", snapshot: null, failure: null });
      record("scan.started", repository.path);
      try {
        const result = await scanProject(repository.path, scanKnobs());
        if (runId !== workspaceScanRun) return;
        setEntry(repository.path, { status: "done", snapshot: result, failure: null });
        record("scan.finished", repository.path, {
          worktrees: result.worktrees.length,
          artifacts: result.artifacts.length,
          sample: false
        });
      } catch (error) {
        if (runId !== workspaceScanRun) return;
        const failure = normaliseScanFailure(error, repository.path);
        setEntry(repository.path, { status: "failed", snapshot: null, failure });
        record("scan.failed", repository.path, {
          error: "detail" in failure ? failure.detail : failure.code
        });
      }
      workspaceScanState.completed = index + 1;
    }
  } finally {
    if (runId === workspaceScanRun) {
      workspaceScanState.running = false;
      workspaceScanState.current = "";
    }
  }
}

/**
 * Opens a snapshot produced by the workspace-wide scan.
 *
 * This is the same overview a single-repository scan leads to, only without a
 * second scan. The repository list remains available, so the reader can come
 * back and open the next result. The build-cache inventory is the folder-scoped
 * one already discovered; records covered by this repository's artifacts are
 * removed exactly as `finishScan` does after a direct scan.
 */
export function openWorkspaceScan(path: string) {
  const entry = workspaceScanEntries.value[path];
  if (executing.value || entry?.status !== "done" || entry.snapshot === null) return;

  clearTimers();
  scanState.running = false;
  scanState.stageIndex = SCAN_STAGES.length - 1;
  scanState.cancelled = false;
  scanFailure.value = null;
  snapshot.value = entry.snapshot;
  projectPath.value = path;
  isDemoData.value = false;
  selectedWorktrees.value = [];
  selectedArtifacts.value = [];
  lastResult.value = [];
  acknowledged.value = false;
  confirmText.value = "";
  recordedPlan = "";
  rememberProject(path);
  applyDefaultSelection();
  if (buildCacheSnapshot.value) {
    const records = withoutProjectArtifacts(buildCacheSnapshot.value.records, entry.snapshot.artifacts);
    if (records.length !== buildCacheSnapshot.value.records.length) {
      buildCacheSnapshot.value = { ...buildCacheSnapshot.value, records };
      applyBuildCacheDefaultSelection();
    }
  }
  view.value = "overview";
}

/**
 * Drops build-cache records the artifact walk already reports.
 *
 * A direct scan searches the repository itself as one of its cache scopes, so a
 * tagged `target/` arrives twice: once as an artifact and once as a build cache.
 * Both are `Safe`, both are selected by default, and the confirmation then
 * promises the same directory and its bytes twice — the second move is skipped at
 * execution because the first has already moved it. The artifact channel owns a
 * path that is already an artifact; the build-cache channel is for roots the
 * artifact walk does not reach.
 */
export function withoutProjectArtifacts(
  records: ArtifactRecord[],
  artifacts: ArtifactRecord[]
): ArtifactRecord[] {
  if (artifacts.length === 0) return records;
  const fold = hostOs() === "windows";
  const covered = new Set(artifacts.map((artifact) => pathIdentityKey(artifact.path, fold)));
  return records.filter((record) => !covered.has(pathIdentityKey(record.path, fold)));
}

function finishScan(result: ProjectSnapshot, sample = false) {
  clearTimers();
  scanState.running = false;
  scanState.stageIndex = SCAN_STAGES.length - 1;
  snapshot.value = result;
  workspaceSnapshot.value = null;
  isDemoData.value = sample;
  rememberProject(projectPath.value);
  record("scan.finished", projectPath.value, {
    worktrees: result.worktrees.length,
    artifacts: result.artifacts.length,
    sample
  });
  applyDefaultSelection();
  // The build-cache stage ran before this snapshot existed, so its inventory could
  // still hold a root that is also an artifact of this repository.
  if (buildCacheSnapshot.value) {
    const records = withoutProjectArtifacts(buildCacheSnapshot.value.records, result.artifacts);
    if (records.length !== buildCacheSnapshot.value.records.length) {
      buildCacheSnapshot.value = { ...buildCacheSnapshot.value, records };
      applyBuildCacheDefaultSelection();
    }
  }
  view.value = "overview";
  if (sample && !buildCacheSnapshot.value) {
    buildCacheSnapshot.value = mockGeneratedCacheSnapshot;
    buildCacheRoot.value = mockGeneratedCacheSnapshot.root;
    buildCacheSample.value = true;
    applyBuildCacheDefaultSelection();
  }
}

export function cancelScan() {
  clearTimers();
  scanRun += 1;
  buildCacheRun += 1;
  buildCacheScanning.value = false;
  scanState.running = false;
  scanState.cancelled = true;
  record("scan.cancelled", projectPath.value, { elapsed: scanState.elapsed });
  view.value = "welcome";
}

export function resetToWelcome() {
  if (executing.value) return;
  stopWorkspaceScan();
  workspaceScanEntries.value = {};
  clearTimers();
  scanState.running = false;
  snapshot.value = null;
  workspaceSnapshot.value = null;
  projectPath.value = "";
  selectedWorktrees.value = [];
  selectedArtifacts.value = [];
  lastResult.value = [];
  scanFailure.value = null;
  executeError.value = null;
  logError.value = null;
  recordedPlan = "";
  view.value = "welcome";
}

/**
 * Writes the plan the reader is looking at into the activity log.
 *
 * The plan is derived state: every checkbox makes a different one, and a line per
 * toggle is a log nobody reads. The moment worth keeping is the one where the
 * reader stops assembling and starts reading — the plan page — because that is the
 * plan they then agree to, and the operation log that follows is what it became.
 * Without it, "how much did it promise, and what was actually freed" is only
 * answerable by re-deriving the plan from a snapshot that has since changed.
 */
function recordPlanGenerated(): void {
  const plan = cleanupPlan.value;
  const cachePlan = buildCachePlan.value;
  // The figure the confirmation line carries, kept the same way on purpose: an
  // artifact that leaves with its worktree is not a separate move, and counting it
  // as one would make the two lines disagree about the same plan.
  const artifacts =
    plan.trash_moves.filter((item) => item.bundled_with_worktree === null).length +
    cachePlan.trash_moves.length;
  if (plan.worktree_removals.length + artifacts === 0) return;

  const signature = [
    ...plan.worktree_removals.map((item) => item.path),
    ...plan.trash_moves.map((item) => item.path),
    ...cachePlan.trash_moves.map((item) => item.path),
    String(plan.total_bytes + cachePlan.total_bytes)
  ].join("\u0000");
  if (signature === recordedPlan) return;
  recordedPlan = signature;

  record("plan.generated", snapshot.value?.root ?? buildCacheSnapshot.value?.root ?? projectPath.value, {
    worktrees: plan.worktree_removals.length,
    artifacts,
    total_bytes: plan.total_bytes + cachePlan.total_bytes
  });
}

/** Records that the plan left the app as JSON — the other way a run can start. */
export function recordPlanExport(itemCount: number): void {
  record("plan.exported", snapshot.value?.root ?? projectPath.value, {
    format: "json",
    item_count: itemCount
  });
}

export function goTo(next: ViewKey) {
  if (executing.value) return;
  if (next === "scanning") return;
  if (next !== "workspace") cancelWorkspaceScan();
  if (next === "workspace" && !isDesktop() && workspaceSnapshot.value === null) {
    workspaceSnapshot.value = mockWorkspaceSnapshot;
    projectPath.value = mockWorkspaceSnapshot.root;
  }
  if (next === "buildCaches" && !isDesktop() && buildCacheSnapshot.value === null) {
    buildCacheSnapshot.value = mockGeneratedCacheSnapshot;
    buildCacheRoot.value = mockGeneratedCacheSnapshot.root;
    buildCacheSample.value = true;
    applyBuildCacheDefaultSelection();
  }
  if (next === "history") void refreshHistory();
  if (next === "plan") recordPlanGenerated();
  view.value = next;
}

export function canOpen(key: ViewKey): boolean {
  if (key === "welcome" || key === "settings" || key === "caches" || key === "buildCaches") return true;
  if (key === "workspace") return workspaceSnapshot.value !== null || !isDesktop();
  if (key === "retire") return snapshot.value?.clone_retirement.eligible === true;
  if (key === "result") return lastResult.value.length > 0;
  // A retired clone has no snapshot left, but its result and audit log are the
  // record of what happened; the history page must remain reachable.
  if (key === "history") return snapshot.value !== null || lastResult.value.length > 0;
  return snapshot.value !== null;
}

export function applyDefaultSelection() {
  selectedWorktrees.value = [];
  selectedArtifacts.value = [];
  selectAllSafe();
}

/**
 * Adds every currently Safe record to the selection without clearing an explicit
 * Review selection. The overview uses this when a cleanup has cleared the
 * selection and the remaining Safe records are still available.
 */
export function selectAllSafe() {
  const snap = snapshot.value;
  if (!snap) return;
  const safeWorktrees = snap.worktrees
    .filter((w) => w.safety.status === "Safe")
    .map((w) => w.path);
  const safeArtifacts = snap.artifacts
    .filter((a) => a.safety.status === "Safe")
    // An unmeasurable size is not "below the threshold": hiding it would silently
    // drop an item the user cannot assess (and `visibleArtifacts` keeps it for the
    // same reason).
    .filter((a) => a.size_bytes === null || a.size_bytes >= settings.min_artifact_mb * 1024 * 1024)
    .map((a) => a.path);

  selectedWorktrees.value = [...new Set([...selectedWorktrees.value, ...safeWorktrees])];
  selectedArtifacts.value = [...new Set([...selectedArtifacts.value, ...safeArtifacts])];
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

/**
 * High-risk confirmation thresholds (D13).
 *
 * This is the only place these numbers are written down. The confirmation
 * reasons are localized, so they receive the thresholds as arguments rather
 * than repeating them: a translation cannot quote a stale "5 items" or
 * "10 GB" while the code is enforcing something else.
 */
export const ACK_COUNT_THRESHOLD = 5;
export const ACK_SIZE_THRESHOLD_BYTES = 10 * 1024 * 1024 * 1024;

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

  const worktree_removals = selectedWorktreeRemovals
    .filter((w) => !blocked.has(w.path))
    .sort(
      (a, b) => compareSizeNullsLast(a.size_bytes, b.size_bytes) || a.path.localeCompare(b.path)
    );
  const trash_moves = selectedTrashMoves
    .filter((t) => !blocked.has(t.path))
    .sort(
      (a, b) => compareSizeNullsLast(a.size_bytes, b.size_bytes) || a.path.localeCompare(b.path)
    );

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
  // Formatted here so the localized copy can quote a limit without knowing the
  // unit, and so the unit follows the active language.
  const sizeLimit = formatBytes(ACK_SIZE_THRESHOLD_BYTES);
  if (forcedItems.length > 0) {
    ack_reasons.push(messages.plan.ack.force(forcedItems.length));
  }
  if (independentItems >= ACK_COUNT_THRESHOLD) {
    ack_reasons.push(messages.plan.ack.count(independentItems));
  }
  if (total_bytes_unknown > 0) {
    // With unknown sizes in the plan the total is a lower bound, so the size
    // threshold below cannot be evaluated — this reason gates the run instead.
    ack_reasons.push(messages.plan.ack.unmeasurable(total_bytes_unknown));
  }
  if (total_bytes > ACK_SIZE_THRESHOLD_BYTES) {
    // The *known* part already exceeds the threshold, which is true regardless of
    // what the unmeasurable items turn out to be.
    ack_reasons.push(messages.plan.ack.size(sizeLimit));
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

export const cacheSelectedSummary = computed(() => {
  const records = (cacheSnapshot.value?.records ?? []).filter((record) =>
    selectedCaches.value.includes(record.path)
  );
  return {
    count: records.length,
    bytes: records.reduce((sum, record) => sum + (record.size_bytes ?? 0), 0),
    unknown: records.filter((record) => record.size_bytes === null).length
  };
});

/** The selected cache directories as the ordinary plan object. */
export const cachePlan = computed<CleanupPlan>(() => {
  const empty: CleanupPlan = {
    worktree_removals: [],
    trash_moves: [],
    excluded_unsafe: 0,
    total_bytes: 0,
    total_bytes_unknown: 0,
    blockers: [],
    ack_reasons: []
  };
  const records = cacheSnapshot.value?.records ?? [];
  if (records.length === 0) return empty;

  const blocked = new Set(
    records
      .filter((record) => selectedCaches.value.includes(record.path))
      .filter((record) => settings.whitelist_paths.some((rule) => pathMatchesRule(record.path, rule)))
      .map((record) => record.path)
  );
  const selected = records.filter(
    (record) => selectedCaches.value.includes(record.path) && !blocked.has(record.path)
  );
  const moves: TrashMoveItem[] = selected
    .map((record) => ({
      kind: "artifact" as const,
      path: record.path,
      artifact_kind: record.kind,
      size_bytes: record.size_bytes,
      file_count: record.file_count,
      bundled_with_worktree: null
    }))
    .sort(
      (a, b) => compareSizeNullsLast(a.size_bytes, b.size_bytes) || a.path.localeCompare(b.path)
    );
  const total_bytes = moves.reduce((sum, move) => sum + (move.size_bytes ?? 0), 0);
  const total_bytes_unknown = moves.filter((move) => move.size_bytes === null).length;
  const ack_reasons: string[] = [];
  if (moves.length >= ACK_COUNT_THRESHOLD) {
    ack_reasons.push(messages.plan.ack.count(moves.length));
  }
  if (total_bytes_unknown > 0) {
    ack_reasons.push(messages.plan.ack.unmeasurable(total_bytes_unknown));
  }
  if (total_bytes > ACK_SIZE_THRESHOLD_BYTES) {
    ack_reasons.push(messages.plan.ack.size(formatBytes(ACK_SIZE_THRESHOLD_BYTES)));
  }
  return {
    worktree_removals: [],
    trash_moves: moves,
    excluded_unsafe: records.filter((record) => record.safety.status === "Unsafe").length,
    total_bytes,
    total_bytes_unknown,
    blockers: [...blocked].map((path) => messages.plan.blocker(path)),
    ack_reasons
  };
});

export const buildCacheSelectedSummary = computed(() => {
  const records = (buildCacheSnapshot.value?.records ?? []).filter((record) =>
    selectedBuildCaches.value.includes(record.path)
  );
  return {
    count: records.length,
    bytes: records.reduce((sum, record) => sum + (record.size_bytes ?? 0), 0),
    unknown: records.filter((record) => record.size_bytes === null).length
  };
});

/** The selected tagged build roots as the ordinary Trash plan object. */
export const buildCachePlan = computed<CleanupPlan>(() => {
  const empty: CleanupPlan = {
    worktree_removals: [],
    trash_moves: [],
    excluded_unsafe: 0,
    total_bytes: 0,
    total_bytes_unknown: 0,
    blockers: [],
    ack_reasons: []
  };
  const records = buildCacheSnapshot.value?.records ?? [];
  if (records.length === 0) return empty;

  const blocked = new Set(
    records
      .filter((record) => selectedBuildCaches.value.includes(record.path))
      .filter((record) => settings.whitelist_paths.some((rule) => pathMatchesRule(record.path, rule)))
      .map((record) => record.path)
  );
  const selected = records.filter(
    (record) => selectedBuildCaches.value.includes(record.path) && !blocked.has(record.path)
  );
  const moves: TrashMoveItem[] = selected
    .map((record) => ({
      kind: "artifact" as const,
      path: record.path,
      artifact_kind: record.kind,
      size_bytes: record.size_bytes,
      file_count: record.file_count,
      bundled_with_worktree: null
    }))
    .sort(
      (a, b) => compareSizeNullsLast(a.size_bytes, b.size_bytes) || a.path.localeCompare(b.path)
    );
  const total_bytes = moves.reduce((sum, move) => sum + (move.size_bytes ?? 0), 0);
  const total_bytes_unknown = moves.filter((move) => move.size_bytes === null).length;
  const ack_reasons: string[] = [];
  if (moves.length >= ACK_COUNT_THRESHOLD) {
    ack_reasons.push(messages.plan.ack.count(moves.length));
  }
  if (total_bytes_unknown > 0) {
    ack_reasons.push(messages.plan.ack.unmeasurable(total_bytes_unknown));
  }
  if (total_bytes > ACK_SIZE_THRESHOLD_BYTES) {
    ack_reasons.push(messages.plan.ack.size(formatBytes(ACK_SIZE_THRESHOLD_BYTES)));
  }
  return {
    worktree_removals: [],
    trash_moves: moves,
    excluded_unsafe: records.filter((record) => record.safety.status === "Unsafe").length,
    total_bytes,
    total_bytes_unknown,
    blockers: [...blocked].map((path) => messages.plan.blocker(path)),
    ack_reasons
  };
});

/** Selects every Safe build root above the artifact threshold. */
export function applyBuildCacheDefaultSelection() {
  const minBytes = settings.min_artifact_mb * 1024 * 1024;
  selectedBuildCaches.value = (buildCacheSnapshot.value?.records ?? [])
    .filter((record) => record.safety.status === "Safe")
    .filter((record) => record.size_bytes === null || record.size_bytes >= minBytes)
    .map((record) => record.path);
  buildCacheAcknowledged.value = false;
  acknowledged.value = false;
}

/** Selects every known-safe cache, leaving an unknown owner for the reader. */
export function applyCacheDefaultSelection() {
  const minBytes = settings.min_artifact_mb * 1024 * 1024;
  selectedCaches.value = (cacheSnapshot.value?.records ?? [])
    .filter((record) => record.safety.status === "Safe")
    .filter((record) => record.size_bytes === null || record.size_bytes >= minBytes)
    .map((record) => record.path);
  cacheAcknowledged.value = false;
}

export function toggleCache(path: string) {
  const record = cacheSnapshot.value?.records.find((item) => item.path === path);
  if (!record || record.safety.status === "Unsafe") return;
  selectedCaches.value = selectedCaches.value.includes(path)
    ? selectedCaches.value.filter((item) => item !== path)
    : [...selectedCaches.value, path];
  cacheAcknowledged.value = false;
}

export function toggleGroupCaches(paths: string[], select: boolean) {
  const allowed = new Set(
    paths.filter((path) => {
      const record = cacheSnapshot.value?.records.find((item) => item.path === path);
      return record && record.safety.status !== "Unsafe";
    })
  );
  const next = new Set(selectedCaches.value);
  for (const path of allowed) {
    if (select) next.add(path);
    else next.delete(path);
  }
  selectedCaches.value = [...next];
  cacheAcknowledged.value = false;
}

export function toggleBuildCache(path: string) {
  const record = buildCacheSnapshot.value?.records.find((item) => item.path === path);
  if (!record || record.safety.status === "Unsafe") return;
  selectedBuildCaches.value = selectedBuildCaches.value.includes(path)
    ? selectedBuildCaches.value.filter((item) => item !== path)
    : [...selectedBuildCaches.value, path];
  buildCacheAcknowledged.value = false;
  acknowledged.value = false;
}

export function toggleGroupBuildCaches(paths: string[], select: boolean) {
  const allowed = new Set(
    paths.filter((path) => {
      const record = buildCacheSnapshot.value?.records.find((item) => item.path === path);
      return record && record.safety.status !== "Unsafe";
    })
  );
  const next = new Set(selectedBuildCaches.value);
  for (const path of allowed) {
    if (select) next.add(path);
    else next.delete(path);
  }
  selectedBuildCaches.value = [...next];
  buildCacheAcknowledged.value = false;
  acknowledged.value = false;
}

/**
 * Inventories tagged build roots below one folder. It never deletes anything.
 *
 * This is the part shared by the normal scan and the page's optional refresh; it
 * deliberately has no `scanState.running` guard so the scan that already owns
 * the scanning page can await it. The exported entry point below is the guarded
 * one.
 */
async function runBuildCacheScan(root: string, searchAncestors = false) {
  const runId = ++buildCacheRun;
  buildCacheScanning.value = true;
  buildCacheError.value = false;
  buildCacheLastResult.value = [];
  buildCacheResultSummary.value = {
    freed: 0,
    freed_unknown: 0,
    trash_bytes: 0,
    trash_unknown: 0,
    reclaimable: 0,
    reclaimable_unknown: 0,
    trashed: 0,
    removed: 0,
    failed: 0
  };
  buildCacheAcknowledged.value = false;
  acknowledged.value = false;
  if (!isDesktop()) {
    buildCacheSnapshot.value = mockGeneratedCacheSnapshot;
    buildCacheRoot.value = mockGeneratedCacheSnapshot.root;
    buildCacheSample.value = true;
    applyBuildCacheDefaultSelection();
    buildCacheScanning.value = false;
    return;
  }
  // A folder-scoped inventory is replaced as a whole. Keeping the previous
  // folder's rows while a new scope is being scanned would pair a failed scan's
  // error with the old records, and could leave those old paths selectable.
  buildCacheSnapshot.value = null;
  selectedBuildCaches.value = [];
  buildCacheSample.value = false;
  record("scan.started", root);
  try {
    const options = {
      per_dir_timeout_ms: Math.max(1, settings.per_dir_timeout_sec * 1000),
      recent_activity_days: settings.active_days,
      whitelist: settings.whitelist_paths
    };
    const scopes = searchAncestors ? cacheSearchScopes(root) : [root];
    const records = new Map<string, ArtifactRecord>();
    let result: GeneratedCacheSnapshot | null = null;
    let recordScope = "";
    let warnings: string[] = [];
    for (const scope of scopes) {
      const candidate = await scanGeneratedCaches(scope, options);
      if (runId !== buildCacheRun) return;
      result = candidate;
      for (const record of candidate.records) {
        // Windows returns a path built from the scope's spelling plus a native
        // separator, so the same cache can arrive as `C:/root/cache` from one
        // scope and `C:/root\cache` from another. Compare normalised keys, not
        // the raw display strings, or the inventory counts it twice.
        records.set(normalizePathKey(record.path), record);
      }
      if (candidate.records.length > 0) {
        if (recordScope === "") recordScope = candidate.root;
        warnings = [...warnings, ...candidate.warnings];
      }
    }
    if (runId !== buildCacheRun) return;
    if (result === null) throw new Error("build-cache scan returned no result");
    const merged: GeneratedCacheSnapshot = {
      ...result,
      root: recordScope || result.root,
      // A refresh runs with the project snapshot in hand; the initial scan runs
      // before it, and `finishScan` applies the same filter once the snapshot
      // exists.
      records: withoutProjectArtifacts([...records.values()], snapshot.value?.artifacts ?? []),
      // An empty parent is not a warning once a child supplied caches. If nothing
      // was found anywhere, keep the last scan's honest "no tagged roots" note.
      warnings: records.size > 0 ? warnings : result.warnings
    };
    buildCacheSnapshot.value = merged;
    buildCacheRoot.value = merged.root;
    buildCacheSample.value = false;
    applyBuildCacheDefaultSelection();
    record("scan.finished", merged.root, {
      worktrees: 0,
      artifacts: merged.records.length,
      sample: false
    });
  } catch {
    if (runId !== buildCacheRun) return;
    buildCacheError.value = true;
    record("scan.failed", root, { error: "build_cache_scan_failed" });
  } finally {
    if (runId === buildCacheRun) buildCacheScanning.value = false;
  }
}

/** Refreshes the build-cache inventory from the folder already selected. */
export async function startBuildCacheScan() {
  if (buildCacheScanning.value || executing.value || scanState.running) return;
  const root = buildCacheRoot.value.trim();
  if (!root && isDesktop()) {
    buildCacheError.value = false;
    return;
  }
  await runBuildCacheScan(root, true);
}

/** Runs the selected build-root moves after their own re-check. */
export async function executeBuildCacheCleanup() {
  const plan = buildCachePlan.value;
  const current = buildCacheSnapshot.value;
  if (!current || (plan.trash_moves.length === 0 && plan.blockers.length === 0)) return;
  if (!isDesktop()) {
    executeError.value = "desktop-only";
    return;
  }
  if (plan.ack_reasons.length > 0 && !buildCacheAcknowledged.value) return;

  executing.value = true;
  executeError.value = null;
  logError.value = null;
  record("cleanup.confirmed", current.root, {
    worktrees: 0,
    artifacts: plan.trash_moves.length,
    forced: 0,
    typed_name: false
  });
  try {
    const outcome = await executeGeneratedCachePlan(plan, current, {
      recent_activity_days: settings.active_days,
      untracked_as_unsafe: false,
      whitelist: settings.whitelist_paths,
      per_dir_timeout_ms: Math.max(1, settings.per_dir_timeout_sec * 1000),
      git_timeout_ms: GIT_TIMEOUT_MS,
      log_path: await logPath()
    });
    const removed = new Set(
      outcome.entries
        .filter((entry) => entry.result === "ok" && entry.action === "trash-move")
        .map((entry) => entry.target)
    );
    buildCacheSnapshot.value = {
      ...current,
      records: current.records.filter((record) => !removed.has(record.path))
    };
    selectedBuildCaches.value = [];
    buildCacheLastResult.value = outcome.entries;
    buildCacheResultSummary.value = summarise(outcome.entries, plan);
    logError.value = outcome.log_error;
    buildCacheAcknowledged.value = false;
    await refreshHistory();
  } catch {
    executeError.value = "failed";
  } finally {
    executing.value = false;
  }
}

/** Inventories the known system-cache roots. It never deletes anything. */
export async function startCacheScan() {
  if (cacheScanning.value || executing.value || scanState.running) return;
  cacheScanning.value = true;
  cacheError.value = false;
  cacheLastResult.value = [];
  cacheResultSummary.value = {
    freed: 0,
    freed_unknown: 0,
    trash_bytes: 0,
    trash_unknown: 0,
    reclaimable: 0,
    reclaimable_unknown: 0,
    trashed: 0,
    removed: 0,
    failed: 0
  };
  cacheAcknowledged.value = false;
  if (!isDesktop()) {
    cacheSnapshot.value = mockCacheSnapshot;
    cacheSample.value = true;
    applyCacheDefaultSelection();
    cacheScanning.value = false;
    return;
  }
  record("scan.started", "System caches");
  try {
    const result = await scanCaches({
      per_dir_timeout_ms: Math.max(1, settings.per_dir_timeout_sec * 1000),
      recent_activity_days: settings.active_days,
      whitelist: settings.whitelist_paths
    });
    cacheSnapshot.value = result;
    cacheSample.value = false;
    applyCacheDefaultSelection();
    record("scan.finished", "System caches", {
      worktrees: 0,
      artifacts: result.records.length,
      sample: false
    });
  } catch {
    cacheError.value = true;
    record("scan.failed", "System caches", { error: "cache_scan_failed" });
  } finally {
    cacheScanning.value = false;
  }
}

/** Runs the selected cache moves after their own re-check. */
export async function executeCacheCleanup() {
  const plan = cachePlan.value;
  const current = cacheSnapshot.value;
  if (!current || (plan.trash_moves.length === 0 && plan.blockers.length === 0)) return;
  if (!isDesktop()) {
    executeError.value = "desktop-only";
    return;
  }
  if (plan.ack_reasons.length > 0 && !cacheAcknowledged.value) return;

  executing.value = true;
  executeError.value = null;
  logError.value = null;
  record("cleanup.confirmed", "System caches", {
    worktrees: 0,
    artifacts: plan.trash_moves.length,
    forced: 0,
    typed_name: false
  });
  try {
    const outcome = await executeCachePlan(plan, current, {
      recent_activity_days: settings.active_days,
      untracked_as_unsafe: false,
      whitelist: settings.whitelist_paths,
      per_dir_timeout_ms: Math.max(1, settings.per_dir_timeout_sec * 1000),
      git_timeout_ms: GIT_TIMEOUT_MS,
      log_path: await logPath()
    });
    const removed = new Set(
      outcome.entries
        .filter((entry) => entry.result === "ok" && entry.action === "trash-move")
        .map((entry) => entry.target)
    );
    cacheSnapshot.value = {
      ...current,
      records: current.records.filter((record) => !removed.has(record.path))
    };
    selectedCaches.value = [];
    cacheLastResult.value = outcome.entries;
    cacheResultSummary.value = summarise(outcome.entries, plan);
    logError.value = outcome.log_error;
    cacheAcknowledged.value = false;
    await refreshHistory();
  } catch {
    executeError.value = "failed";
  } finally {
    executing.value = false;
  }
}

/**
 * Runs the confirmed plan.
 *
 * The project channels and the selected build caches are one confirmation but
 * two executors: a worktree/artifact plan is anchored to one repository, while a
 * build-cache plan is anchored to tagged directories under the scanned folder.
 * They are re-checked and reported separately, then merged into one result table.
 * Nothing is simulated: a skipped item stays skipped and a failure stays visible.
 */
export async function executeCleanup() {
  const plan = cleanupPlan.value;
  const cachePlan = buildCachePlan.value;
  const current = snapshot.value;
  const cacheCurrent = buildCacheSnapshot.value;
  const projectHasItems =
    plan.worktree_removals.length > 0 ||
    plan.trash_moves.length > 0 ||
    plan.blockers.length > 0;
  const cacheHasItems = cachePlan.trash_moves.length > 0;
  if (!projectHasItems && !cacheHasItems) return;
  const ackReasons = [...new Set([...plan.ack_reasons, ...cachePlan.ack_reasons])];

  if (!isDesktop()) {
    // The preview cannot reach the file system, and there is no honest way to
    // pretend otherwise: it says why instead of showing a fake result page.
    executeError.value = "desktop-only";
    return;
  }

  executing.value = true;
  executeError.value = null;
  logError.value = null;
  // What the reader confirmed, not what happened afterwards: the deletions are
  // already in the operation log, and this line is what they agreed to.
  record("cleanup.confirmed", current?.root ?? cacheCurrent?.root ?? projectPath.value, {
    worktrees: plan.worktree_removals.length,
    artifacts:
      plan.trash_moves.filter((item) => item.bundled_with_worktree === null).length +
      cachePlan.trash_moves.length,
    forced: plan.worktree_removals.filter((item) => item.force).length,
    typed_name: confirmText.value.trim() !== ""
  });
  executionProgress.value = [];
  let unlisten: (() => void) | null = null;
  try {
    let projectOutcome: ExecutionOutcome | null = null;
    let cacheOutcome: ExecutionOutcome | null = null;
    let executorFailed = false;
    if (projectHasItems && current) {
      try {
        // The live list is an enhancement, not a condition for running: if the
        // event channel is unavailable, the final outcome still carries every row.
        unlisten = await onExecutionProgress((entry) => {
          executionProgress.value = [...executionProgress.value, entry];
        }).catch(() => null);
        projectOutcome = await executePlan(plan, current, {
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
      } catch {
        executorFailed = true;
      }
    }

    if (cacheHasItems && cacheCurrent) {
      try {
        cacheOutcome = await executeGeneratedCachePlan(cachePlan, cacheCurrent, {
          recent_activity_days: settings.active_days,
          untracked_as_unsafe: false,
          whitelist: settings.whitelist_paths,
          per_dir_timeout_ms: Math.max(1, settings.per_dir_timeout_sec * 1000),
          git_timeout_ms: GIT_TIMEOUT_MS,
          log_path: await logPath()
        });
        applyBuildCacheRemovals(cacheOutcome, cachePlan);
      } catch {
        executorFailed = true;
      }
    }

    const combinedPlan: CleanupPlan = {
      ...plan,
      trash_moves: [...plan.trash_moves, ...cachePlan.trash_moves],
      blockers: [...plan.blockers, ...cachePlan.blockers],
      ack_reasons: ackReasons,
      total_bytes: plan.total_bytes + cachePlan.total_bytes,
      total_bytes_unknown: plan.total_bytes_unknown + cachePlan.total_bytes_unknown
    };
    const entries = [...(projectOutcome?.entries ?? []), ...(cacheOutcome?.entries ?? [])];
    const logErrors = [projectOutcome?.log_error, cacheOutcome?.log_error].filter(
      (error): error is string => error !== null && error !== undefined
    );
    if (projectOutcome !== null || cacheOutcome !== null) {
      await applyOutcome(entries, combinedPlan, logErrors.join("; ") || null);
    }
    if (executorFailed) executeError.value = "failed";
  } catch (error) {
    executeError.value = "failed";
  } finally {
    unlisten?.();
    executing.value = false;
  }
}

/**
 * Retires one whole clone.
 *
 * The core owns the decision and re-checks it. The front end only supplies the
 * scan that was on screen, the same executor knobs as a project cleanup, and the
 * operation-log path. On success the whole clone is in the Trash; the snapshot is
 * cleared because its root no longer exists, while the result rows and audit trail
 * remain visible.
 */
export async function executeRetireClone() {
  const current = snapshot.value;
  if (!current || !current.clone_retirement.eligible || executing.value) return;
  if (!isDesktop()) {
    executeError.value = "desktop-only";
    return;
  }

  executing.value = true;
  executeError.value = null;
  logError.value = null;
  const summaryPlan: CleanupPlan = {
    worktree_removals: [],
    trash_moves: [],
    excluded_unsafe: 0,
    total_bytes: 0,
    total_bytes_unknown: 0,
    blockers: [],
    ack_reasons: []
  };
  record("cleanup.confirmed", current.root, {
    worktrees: current.worktrees.filter((record) => !record.is_main).length,
    artifacts: 0,
    forced: 0,
    typed_name: true
  });
  try {
    const outcome = await retireClone(current, {
      recent_activity_days: settings.active_days,
      untracked_as_unsafe: settings.untracked_is_unsafe,
      whitelist: settings.whitelist_paths,
      per_dir_timeout_ms: Math.max(1, settings.per_dir_timeout_sec * 1000),
      git_timeout_ms: GIT_TIMEOUT_MS,
      log_path: await logPath()
    });
    const retired = outcome.entries.some(
      (entry) => entry.action === "clone-retire" && entry.result === "ok"
    );
    lastResult.value = outcome.entries;
    resultSummary.value = summarise(outcome.entries, summaryPlan);
    logError.value = outcome.log_error;
    projectPath.value = current.root;
    if (retired) {
      // The root no longer exists, and keeping it in the workspace would offer a
      // row that cannot be opened. The result page keeps the record of what left.
      snapshot.value = null;
      if (workspaceSnapshot.value) {
        const repositories = workspaceSnapshot.value.repositories.filter(
          (repository) => repository.path !== current.root
        );
        workspaceSnapshot.value =
          repositories.length > 0
            ? { ...workspaceSnapshot.value, repositories }
            : null;
        workspaceScanEntries.value = Object.fromEntries(
          Object.entries(workspaceScanEntries.value).filter(([path]) => path !== current.root)
        );
      }
      buildCacheRoot.value = "";
      buildCacheSnapshot.value = null;
      selectedBuildCaches.value = [];
      selectedWorktrees.value = [];
      selectedArtifacts.value = [];
      forgetProject(current.root);
    }
    view.value = "result";
    await refreshHistory();
  } catch {
    executeError.value = "failed";
  } finally {
    executing.value = false;
  }
}

/** Updates the build-cache inventory and its own result panel after a combined run. */
function applyBuildCacheRemovals(outcome: ExecutionOutcome, plan: CleanupPlan) {
  const current = buildCacheSnapshot.value;
  if (!current) return;
  const removed = new Set(
    outcome.entries
      .filter((entry) => entry.result === "ok" && entry.action === "trash-move")
      .map((entry) => entry.target)
  );
  buildCacheSnapshot.value = {
    ...current,
    records: current.records.filter((record) => !removed.has(record.path))
  };
  selectedBuildCaches.value = [];
  buildCacheLastResult.value = outcome.entries;
  buildCacheResultSummary.value = summarise(outcome.entries, plan);
  buildCacheAcknowledged.value = false;
}

/**
 * Applies the executor's removals to the snapshot on screen.
 *
 * The scan is a photograph of the moment it ran, and the executor's answer is the
 * only authoritative record of what actually left. Keeping the removed rows makes
 * the sidebar keep counting files that are already gone; filtering them here is
 * what makes the numbers follow the cleanup. The next full scan is still the
 * authority on everything that was skipped, which is why the selection is cleared
 * rather than left pointing at rows whose safety was re-checked at execution time.
 */
function applyRemovalsToSnapshot(entries: ResultEntry[], plan: CleanupPlan) {
  const current = snapshot.value;
  if (!current) return;

  // A prune entry names the repository, not the worktree: `git worktree prune`
  // runs once in the main worktree and cleans every stale registration. Removing
  // the entry's target would remove the main working tree from the screenshot, so
  // the pruned paths come from the plan items whose command is the prune.
  const pruned = entries.some(
    (entry) => entry.action === "prune" && entry.result === "ok"
  )
    ? plan.worktree_removals
        .filter((item) => item.command.startsWith("git worktree prune"))
        .map((item) => item.path)
    : [];
  const removedWorktrees = new Set([
    ...entries
      .filter((entry) => entry.result === "ok" && entry.action === "worktree-remove")
      .map((entry) => entry.target),
    ...pruned
  ]);
  const removedArtifacts = new Set(
    entries
      .filter((entry) => entry.result === "ok" && entry.action === "trash-move")
      .map((entry) => entry.target)
  );

  const worktrees = current.worktrees.filter((record) => !removedWorktrees.has(record.path));
  const remainingWorktrees = new Set(worktrees.map((record) => record.path));
  const artifacts = current.artifacts.filter(
    (record) =>
      !removedArtifacts.has(record.path) &&
      (record.worktree_path === null || remainingWorktrees.has(record.worktree_path))
  );

  const updated: ProjectSnapshot = { ...current, worktrees, artifacts };
  snapshot.value = updated;
  const workspaceEntry = workspaceScanEntries.value[updated.root];
  if (workspaceEntry?.status === "done") {
    workspaceScanEntries.value = {
      ...workspaceScanEntries.value,
      [updated.root]: { ...workspaceEntry, snapshot: updated }
    };
  }
  selectedWorktrees.value = [];
  selectedArtifacts.value = [];
}

/** Turns the executor's answer into what the result page shows. */
async function applyOutcome(
  entries: ResultEntry[],
  plan: CleanupPlan,
  logErrorText: string | null
) {
  // A whitelist-blocked item never reaches the executor — it was filtered out of
  // both channels before the plan was built — so it is added back here as its own
  // skipped row. Leaving it out would make the plan page's count disagree with
  // the rows on this one. Read it before the snapshot filter clears the selection.
  const blocked: ResultEntry[] = blockedPaths.value.map((path) => ({
    action: "trash-move",
    target: path,
    result: "skipped",
    detail_code: "precondition.failed",
    detail_args: {},
    detail: null,
    size_bytes: null
  }));

  applyRemovalsToSnapshot(entries, plan);
  lastResult.value = [...entries, ...blocked];
  resultSummary.value = summarise(lastResult.value, plan);
  logError.value = logErrorText;
  acknowledged.value = false;
  confirmText.value = "";
  view.value = "result";
  // Awaited so the history on screen is the log the run just wrote, not the one
  // that was there before it.
  await refreshHistory();
}

/**
 * Counts a run for the result summary.
 *
 * A worktree removal frees bytes now. An independently trashed artifact moves
 * bytes to the Trash: they are recoverable, and are not added to `freed` until
 * the Trash is emptied. A `prune` entry cleans metadata and frees nothing, and a
 * skipped one removed nothing at all. A `null` size counts as unmeasurable rather
 * than as zero (section 3).
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
  let trashBytes = 0;
  let trashUnknown = 0;
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
        if (entry.size_bytes === null) freedUnknown += 1;
        else freed += entry.size_bytes;
        break;
      case "trash-move":
        if (!bundled.has(entry.target)) {
          trashed += 1;
          if (entry.size_bytes === null) trashUnknown += 1;
          else trashBytes += entry.size_bytes;
        }
        break;
      case "prune":
        break;
      case "clone-retire":
        // The clone is neither a worktree nor an artifact; the result page has its
        // own sentence for it. Do not add its unknown size to the artifact Trash
        // totals.
        break;
    }
  }

  return {
    freed,
    freed_unknown: freedUnknown,
    trash_bytes: trashBytes,
    trash_unknown: trashUnknown,
    reclaimable: freed + trashBytes,
    reclaimable_unknown: freedUnknown + trashUnknown,
    trashed,
    removed,
    failed
  };
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
    activity.value = [...mockActivity].reverse();
    historyError.value = false;
    return;
  }
  try {
    // The log is appended oldest first; every page shows the newest on top.
    const [deletions, actions] = await Promise.all([readHistory(), readActivity()]);
    history.value = deletions.reverse();
    activity.value = actions.reverse();
    historyError.value = false;
  } catch {
    historyError.value = true;
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
