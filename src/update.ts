// The update path: check the release feed, download the new build, install it,
// and restart into it.
//
// Everything here is desktop-only. A browser preview has no release feed and no
// way to replace its own binary, so the module says that rather than pretending
// to look. The desktop build is the one that updates itself; the preview is a
// picture of the interface.
//
// The state is a small state machine rather than a bag of booleans, because the
// order matters: a download may only start from "available", and a restart may
// only follow "ready". One `phase` says which of those the reader is looking at.

import { reactive } from "vue";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { hostOs, isDesktop } from "./platform";

export type UpdatePhase =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "error"
  | "unsupported";

export interface UpdateState {
  /** Where the reader is in the update path. */
  phase: UpdatePhase;
  /** The version on offer, when there is one. */
  version: string | null;
  /** The release notes that came with it. */
  notes: string | null;
  /** Bytes downloaded so far, and the total when the server told us one. */
  downloaded: number;
  total: number;
  /** Why the last attempt failed. */
  error: string | null;
  /** When the last check finished, so the page can say how fresh it is. */
  checkedAt: number | null;
}

export const updateState = reactive<UpdateState>({
  phase: "idle",
  version: null,
  notes: null,
  downloaded: 0,
  total: 0,
  error: null,
  checkedAt: null
});

/**
 * The update the check found.
 *
 * Held here rather than in the state because the plugin's `Update` is what
 * downloads: it carries the signature check and the file handle, and the state
 * only ever holds what the reader sees.
 */
let pending: Update | null = null;

/** Whether the app is in the middle of something that must not be interrupted. */
export function isBusy(): boolean {
  return updateState.phase === "checking" || updateState.phase === "downloading";
}

/**
 * Asks the release feed whether there is something newer.
 *
 * Returns quietly on failure: a build that cannot reach the feed is a build that
 * still works, and a banner about a failed check would be noise on every launch
 * behind a restrictive network. The error is kept for the Settings page, which
 * is where somebody who is asking about updates is looking.
 */
export async function checkForUpdates(): Promise<void> {
  if (!isDesktop()) {
    updateState.phase = "unsupported";
    return;
  }
  if (isBusy()) return;
  updateState.phase = "checking";
  updateState.error = null;
  try {
    const found = await check();
    pending = found;
    updateState.version = found?.version ?? null;
    updateState.notes = found?.body ?? null;
    updateState.phase = found ? "available" : "idle";
    updateState.checkedAt = Date.now();
  } catch (error) {
    updateState.phase = "error";
    updateState.error = error instanceof Error ? error.message : String(error);
    updateState.checkedAt = Date.now();
  }
}

/**
 * Downloads the update that was found and installs it.
 *
 * The download is the long part, so the progress is reported as it goes: the
 * bytes are counted against the total the server gave, and a server that gave no
 * total leaves the percentage unknown rather than inventing one.
 */
export async function installUpdate(): Promise<void> {
  if (!pending || isBusy()) return;
  const update = pending;
  updateState.phase = "downloading";
  updateState.downloaded = 0;
  updateState.total = 0;
  updateState.error = null;
  try {
    await update.downloadAndInstall((event) => {
      if (event.event === "Started") {
        updateState.total = event.data.contentLength ?? 0;
      } else if (event.event === "Progress") {
        updateState.downloaded += event.data.chunkLength ?? 0;
      }
    });
    updateState.phase = "ready";
  } catch (error) {
    updateState.phase = "error";
    updateState.error = error instanceof Error ? error.message : String(error);
  }
}

/**
 * Restarts the app so the new build is the one running.
 *
 * On Windows the installer closes the app itself, so the restart there is
 * best-effort: the window may already be gone by the time this runs, and the
 * reader is told to open the app again.
 */
export async function restartApp(): Promise<void> {
  try {
    await relaunch();
  } catch (error) {
    updateState.error = error instanceof Error ? error.message : String(error);
  }
}

/** Whether a restart is the next step, and whether this platform needs one. */
export function restartNeeded(): boolean {
  return updateState.phase === "ready";
}

/** Whether the installer has to close the app itself (Windows does). */
export function installerRestartsApp(): boolean {
  return hostOs() === "windows";
}

/** The share of the download that is done, or `null` when it is not known. */
export function downloadPercent(): number | null {
  if (updateState.total <= 0) return null;
  return Math.min(100, Math.round((updateState.downloaded / updateState.total) * 100));
}
