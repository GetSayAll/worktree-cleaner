// The single place that knows whether the app is running in the desktop shell
// or in a browser preview.
//
// Everything the desktop offers sits behind an IPC call, and the browser has no
// counterpart: a preview can render every page, but it cannot pick a folder,
// read a repository, or move anything to the Trash. Declaring that difference
// once here keeps every view from probing for it, and keeps `invoke` out of the
// components.
//
// The Tauri packages are imported normally rather than lazily. Their module
// bodies only define functions — the host is not touched until one is called, and
// every entry point below checks `isDesktop()` first — so importing them in a
// browser is harmless. It is also required: the browser preview inlines the build
// into a single classic script, and a lazy import makes Vite emit a second chunk
// that such a page cannot load.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { homeDir as resolveHomeDir } from "@tauri-apps/api/path";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath as openWithSystem, revealItemInDir } from "@tauri-apps/plugin-opener";

import type {
  CleanupPlan,
  ExecutionOutcome,
  HistoryEntry,
  ProjectSnapshot,
  ScanOptions
} from "./types";

/** Whether a Tauri backend is reachable at all. */
export function isDesktop(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Raised when a desktop-only action is attempted in a browser preview. */
export class DesktopOnlyError extends Error {
  constructor() {
    super("this action needs the desktop app");
    this.name = "DesktopOnlyError";
  }
}

async function invoke<T>(command: string, args: Record<string, unknown>): Promise<T> {
  if (!isDesktop()) throw new DesktopOnlyError();
  return tauriInvoke<T>(command, args);
}

// ---- scanning and executing -------------------------------------------------

/** Runs the real scan. */
export async function scanProject(
  path: string,
  options: ScanOptions
): Promise<ProjectSnapshot> {
  return invoke<ProjectSnapshot>("scan_project", { path, options });
}

/** The executor's knobs, as the settings page holds them. */
export interface ExecuteKnobs {
  recent_activity_days: number;
  untracked_as_unsafe: boolean;
  whitelist: string[];
  /** Budget for one size measurement during the re-check, in milliseconds. */
  per_dir_timeout_ms: number;
  /** Budget for one Git command during the re-check, in milliseconds. */
  git_timeout_ms: number;
  /** `null` disables the audit log. */
  log_path: string | null;
}

/**
 * Runs a confirmed plan.
 *
 * The snapshot travels with the plan because the executor re-checks against it:
 * a plan that arrives without its scan result is refused rather than executed
 * blind (D11).
 */
export async function executePlan(
  plan: CleanupPlan,
  snapshot: ProjectSnapshot,
  options: ExecuteKnobs
): Promise<ExecutionOutcome> {
  return invoke<ExecutionOutcome>("execute_plan", {
    request: { plan, snapshot, ...options }
  });
}

// ---- the audit log ----------------------------------------------------------

/** Where the operation log lives, or `null` when the platform has no answer. */
export async function logPath(): Promise<string | null> {
  return invoke<string | null>("log_path", {});
}

/** The audit trail, oldest first. An unreadable or missing log is empty. */
export async function readHistory(): Promise<HistoryEntry[]> {
  return invoke<HistoryEntry[]>("read_history", {});
}

/** Empties the log, keeping the file itself. */
export async function clearLog(): Promise<void> {
  await invoke<void>("clear_log", {});
}

// ---- the operating system ---------------------------------------------------

/**
 * Opens the native folder picker.
 *
 * Returns the chosen path, or `null` when the user cancels — and also `null` in
 * a browser preview, where there is no picker to open; the caller is expected to
 * offer the text field as the other way in.
 */
export async function pickDirectory(title: string): Promise<string | null> {
  if (!isDesktop()) return null;
  const picked = await openDialog({ directory: true, multiple: false, title });
  return typeof picked === "string" ? picked : null;
}

/** Shows a path in Finder / Explorer, selecting it. */
export async function revealInFileManager(path: string): Promise<void> {
  if (!isDesktop()) return;
  await revealItemInDir(path);
}

/** Opens a file with whatever the system associates with it. */
export async function openPath(path: string): Promise<void> {
  if (!isDesktop()) return;
  await openWithSystem(path);
}

/**
 * Opens the system Trash / Recycle Bin.
 *
 * Each platform keeps it somewhere different, and there is no shared API for
 * "show me the Trash", so the location is derived from the user agent the webview
 * reports. A failure is swallowed: the caller has nothing better to offer than
 * the path it already shows.
 */
export async function openTrash(): Promise<void> {
  if (!isDesktop()) return;
  const agent = typeof navigator === "undefined" ? "" : navigator.userAgent;
  if (agent.includes("Windows")) {
    // The shell namespace, which Explorer resolves to the Recycle Bin.
    await openPath("shell:RecycleBinFolder");
    return;
  }
  const home = await homeDir();
  if (!home) return;
  const trash = agent.includes("Macintosh")
    ? `${home}/.Trash`
    : `${home}/.local/share/Trash/files`;
  await openPath(trash);
}

/** The user's home directory, used to show `~` in paths. */
export async function homeDir(): Promise<string | null> {
  if (!isDesktop()) return null;
  try {
    return await resolveHomeDir();
  } catch {
    return null;
  }
}

/**
 * Calls `handler` with the paths of anything dropped on the window.
 *
 * Returns the unsubscribe function, or `null` in a browser preview, where the
 * page's own drag-and-drop handling is the only option.
 */
export async function onPathsDropped(
  handler: (paths: string[]) => void
): Promise<(() => void) | null> {
  if (!isDesktop()) return null;
  const unlisten = await getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type === "drop" && event.payload.paths.length > 0) {
      handler(event.payload.paths);
    }
  });
  return unlisten;
}

/**
 * Replaces a leading home directory with `~`, for display only.
 *
 * The path that reaches the core is always the full one: the shell is the only
 * place that expands `~` back, and it does so before anything touches the file
 * system.
 */
export function withTilde(path: string, home: string | null): string {
  if (!home || !path.startsWith(home)) return path;
  return `~${path.slice(home.length)}`;
}
