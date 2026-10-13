import { messages } from "./i18n";

const KB = 1024;

// ---- path helpers ----------------------------------------------------------
//
// Paths reach the UI from macOS, Linux, and Windows, so they may use either the
// POSIX `/` or the Windows `\` separator. Every basename, abbreviation, and rule
// comparison goes through these helpers so a Windows path behaves like a POSIX
// one. Nothing here derives the user's home directory: a browser cannot know it,
// and the value would be wrong on someone else's machine. `homePath` exists only
// to shorten a string for display and is never used for logic.

/** Normalise a path for comparison: unify separators, collapse repeats, drop a trailing separator. */
export function normalizePathKey(path: string): string {
  let key = path.replace(/\\/g, "/").replace(/\/{2,}/g, "/");
  if (key.length > 1) key = key.replace(/\/+$/, "");
  return key;
}

/** Whether the path is absolute on POSIX (`/x`), Windows (`C:\x`), or UNC (`\\host\share`). */
export function isAbsolutePath(path: string): boolean {
  return /^[\\/]/.test(path) || /^[A-Za-z]:[\\/]/.test(path);
}

/** Whether `path` is `rule` itself or lives inside it, compared component by component. */
export function pathMatchesRule(path: string, rule: string): boolean {
  const candidate = normalizePathKey(path).split("/");
  const target = normalizePathKey(rule).split("/");
  if (target.length === 0 || target.length > candidate.length) return false;
  return target.every((segment, index) => segment === candidate[index]);
}

/**
 * Sort records so the ones a user can act on without thinking come first.
 *
 * The order is deliberate and it is the answer to "which of these do I look at":
 * safe before review before unsafe, and within a level, largest first. Sorting by
 * path alone — which is what the lists used to do — scattered the removable items
 * among the forbidden ones and made the user read every row to find the ones that
 * were actually worth removing. Unsafe last also puts everything that cannot be
 * deleted in one glanceable block at the bottom rather than interleaved.
 *
 * `null` sizes sort before measured ones, for the reason
 * {@link compareSizeNullsFirst} gives.
 */
export function compareByActionability(
  a: { safety: { status: string } },
  b: { safety: { status: string } }
): number {
  const rank = { Safe: 0, Review: 1, Unsafe: 2 } as Record<string, number>;
  const diff = (rank[a.safety.status] ?? 3) - (rank[b.safety.status] ?? 3);
  if (diff !== 0) return diff;
  return 0;
}

export function formatBytes(bytes: number | null, opts: { approximate?: boolean } = {}): string {
  if (bytes === null || bytes === undefined) return messages.common.unknown;
  const prefix = opts.approximate ? messages.units.atLeast : "";
  if (bytes < KB) return `${prefix}${bytes} ${messages.units.bytes}`;
  const units = [
    messages.units.kilobytes,
    messages.units.megabytes,
    messages.units.gigabytes,
    messages.units.terabytes
  ];
  let value = bytes / KB;
  let unit = 0;
  while (value >= KB && unit < units.length - 1) {
    value /= KB;
    unit += 1;
  }
  const digits = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${prefix}${value.toFixed(digits)} ${units[unit]}`;
}

export function formatCount(n: number): string {
  return n.toLocaleString("en-US");
}

/**
 * Sort comparator for "largest first" lists that keeps an unmeasurable item at
 * the top instead of burying it at the bottom.
 *
 * Treating `null` as `0` would sort an item whose size could not be measured
 * *below* every measured one — the one item the user cannot assess would be the
 * hardest to find. Unknown sizes come first; measured sizes follow, descending.
 */
export function compareSizeNullsFirst(a: number | null, b: number | null): number {
  if (a === null && b === null) return 0;
  if (a === null) return -1;
  if (b === null) return 1;
  return b - a;
}

export function formatRelTime(iso: string | null): string {
  if (!iso) return messages.common.unknown;
  const diff = Date.now() - new Date(iso).getTime();
  const days = Math.floor(diff / 86400_000);
  if (days <= 0) {
    const hours = Math.floor(diff / 3600_000);
    return hours <= 0 ? messages.time.justNow : messages.time.hoursAgo(hours);
  }
  if (days === 1) return messages.time.oneDayAgo;
  if (days < 30) return messages.time.daysAgo(days);
  const months = Math.floor(days / 30);
  if (months < 12) return messages.time.monthsAgo(months);
  return messages.time.yearsAgo(Math.floor(months / 12));
}

export function formatClock(seconds: number): string {
  return messages.units.seconds(Math.floor(seconds));
}

export function formatTimestamp(iso: string | null): string {
  if (!iso) return messages.common.emDash;
  const d = new Date(iso);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function shortenPath(path: string, root: string): string {
  const key = normalizePathKey(path);
  const rootKey = normalizePathKey(root);
  if (key === rootKey) return ".";
  if (rootKey.length > 0 && key.startsWith(`${rootKey}/`)) return key.slice(rootKey.length + 1);
  return path;
}

export function displayPath(path: string, root: string): string {
  if (normalizePathKey(path) === normalizePathKey(root)) return messages.common.mainWorktree(basename(root));
  return shortenPath(path, root);
}

/**
 * Whether `path` is `root` itself or sits underneath it.
 *
 * For telling the reader about the shape of what they are looking at — a worktree of
 * the repository they picked can live anywhere, and the pages say so. It decides no
 * verdict: the scanner and the executor work from the paths Git reports.
 */
export function isUnder(path: string, root: string): boolean {
  const key = normalizePathKey(path);
  const rootKey = normalizePathKey(root);
  if (rootKey.length === 0) return false;
  return key === rootKey || key.startsWith(`${rootKey}/`);
}

/** Display-only abbreviation of a home directory; tolerates both separators. */
export function homePath(path: string): string {
  return path.replace(/^(?:[A-Za-z]:)?[\\/](?:Users|home)[\\/][^\\/]+/, "~");
}

export function basename(path: string): string {
  const parts = normalizePathKey(path).split("/").filter((part) => part !== "");
  return parts.length > 0 ? parts[parts.length - 1] : path;
}
