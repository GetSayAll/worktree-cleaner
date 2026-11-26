import { messages } from "./i18n";
import type { DetailArg, DetailCode, HygieneFinding, Reason, ReasonCode, ScanFailure } from "./types";

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
  let key = path.replace(/\\/g, "/");
  // A leading double separator is the UNC prefix on Windows (`\\server\share`),
  // not a repeated separator. Collapsing it to `/server/share` turns a network
  // path into a drive-relative one, so preserve the two leading slashes.
  const unc = key.startsWith("//");
  key = key.replace(/\/{2,}/g, "/");
  if (unc) key = `//${key.replace(/^\/+/, "")}`;
  if (key.length > (unc ? 2 : 1)) key = key.replace(/\/+$/, "");
  return key;
}

/** Whether the path is absolute on POSIX (`/x`), Windows (`C:\x`), or UNC (`\\host\share`). */
export function isAbsolutePath(path: string): boolean {
  return /^[\\/]/.test(path) || /^[A-Za-z]:[\\/]/.test(path);
}

/** Whether a path is written in a Windows form, where names are case-insensitive. */
function isWindowsPath(path: string): boolean {
  return /^[A-Za-z]:[\\/]/.test(path) || path.startsWith("\\\\") || path.startsWith("//");
}

function foldSegments(segments: string[], windows: boolean): string[] {
  return windows ? segments.map((segment) => segment.toLowerCase()) : segments;
}

/** Whether `path` is `rule` itself or lives inside it, compared component by component. */
export function pathMatchesRule(path: string, rule: string): boolean {
  const windows = isWindowsPath(path) || isWindowsPath(rule);
  const candidate = foldSegments(normalizePathKey(path).split("/"), windows);
  const target = foldSegments(normalizePathKey(rule).split("/"), windows);
  if (target.length === 0 || target.length > candidate.length) return false;
  return target.every((segment, index) => segment === candidate[index]);
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
 * The reader-facing wording of one safety reason.
 *
 * The core sends a stable code and an English sentence; the code decides what the
 * reader sees. The sentence is only the fallback for a code this build does not
 * know, so a newer core against an older UI says something true instead of
 * nothing. `evidence` is not passed through here: it is a command, a count or a
 * path, and it stays exactly as the core reported it.
 */
export function reasonText(reason: Reason): string {
  return reasonCopy(reason.code) ?? reason.text;
}

/**
 * The wording for a reason code, or `undefined` for a code this build does not
 * know. Split out so a `DetailCode` that carries a reason code as an argument
 * reads the same table rather than building a fake `Reason` to look it up.
 */
export function reasonCopy(code: string): string | undefined {
  const dot = code.indexOf(".");
  const family = code.slice(0, dot) as "wt" | "af";
  const table: Record<string, string> | undefined = messages.reasons[family];
  return table?.[code.slice(dot + 1)];
}

/**
 * Renders one argument of a `DetailCode` the way this language writes it.
 *
 * A byte count arrives as a raw number and is formatted here rather than in the
 * core, which is the whole point of sending it raw: `1.29 GB` must not be
 * spelled once in Rust and then frozen into a Chinese screen. `null` is the
 * absence of a measurement and says so, because "unknown" and "zero" are the
 * difference D3 is about.
 */
export function detailArg(arg: DetailArg | undefined): string {
  if (arg === null || arg === undefined) return messages.common.unknown;
  if (typeof arg === "number") return formatCount(arg);
  if (typeof arg === "boolean") return arg ? messages.detail.yes : messages.detail.no;
  return arg;
}

/**
 * The reader-facing wording of one policy finding.
 *
 * The core sends the rule, the situation and the raw values; which sentence they
 * become belongs to the message table, so a Chinese screen never carries an English
 * decision. A situation this build has no copy for falls back to the evidence line,
 * which is a fact and stays true either way — an id would mean nothing to a reader,
 * and a blank row would look like the finding had gone away.
 */
export function hygieneText(finding: HygieneFinding): string {
  const state = typeof finding.args.state === "string" ? finding.args.state : "";
  const table: Record<string, Record<string, string> | undefined> = messages.hygiene.states;
  const copy = table[hygieneRuleKey(finding.rule)]?.[state];
  if (typeof copy !== "string") return finding.evidence;
  return copy.replace(/\{(\w+)\}/g, (_, name: string) => hygieneArg(name, finding.args[name]));
}

/**
 * The message-table key for a hygiene rule.
 *
 * `B1.1` is the one rule whose id carries a dot, and the locale test walks a
 * table by splitting a leaf path on dots — so a dotted key would be read as one
 * level deeper than it is and the copy would go missing in every language. The
 * table spells it `B11`; the wire format keeps the dot, because that is what the
 * policy document calls it.
 */
export function hygieneRuleKey(rule: string): string {
  return rule.replace(/\./g, "");
}

/**
 * One value of a finding, written the way this language writes it.
 *
 * Byte counts arrive as raw numbers and are formatted here, for the same reason
 * `detailArg` does it: a size spelled in Rust would freeze an English unit into a
 * Chinese screen. The key decides, because the wire format is untagged — `Bytes`,
 * `Count` and `Percent` all arrive as plain numbers.
 */
function hygieneArg(name: string, value: DetailArg | undefined): string {
  if (name.endsWith("_bytes") || name === "size" || name === "available" || name === "total") {
    return typeof value === "number" ? formatBytes(value) : messages.common.unknown;
  }
  if (name === "percent") {
    return typeof value === "number" ? `${formatCount(value)}%` : messages.common.unknown;
  }
  return detailArg(value);
}

/**
 * The wording of one executed action, from its code and arguments.
 *
 * The fallback is the record's own sentence: a log written before the codes
 * existed still reads back, and a core newer than this UI still says something
 * true rather than nothing.
 */
export function detailText(code: DetailCode | null, args: Record<string, DetailArg>, legacy?: string | null): string {
  if (!code) return legacy ?? messages.detail.legacy;
  // Two levels, for the same reason `reasonCopy` is: the table is walked by
  // splitting a path on dots, so a key may not contain one.
  const dot = code.indexOf(".");
  const families: Record<string, Record<string, string> | undefined> = messages.detail.codes;
  const copy = families[code.slice(0, dot)]?.[code.slice(dot + 1)];
  // A code with no entry in the table would otherwise print the raw id, which is
  // the one thing worse than English: it means nothing to a reader.
  if (typeof copy !== "string") return legacy ?? messages.detail.legacy;
  const fields: Record<string, string> = messages.detail.fields;
  const reasonCode = typeof args.reason === "string" ? reasonCopy(args.reason) : undefined;
  return copy
    .replace("{field}", typeof args.field === "string" ? fields[args.field] ?? "" : "")
    .replace("{before}", detailArg(args.before))
    .replace("{after}", detailArg(args.after))
    .replace("{reason}", reasonCode ?? messages.common.unknown)
    .replace("{pruneError}", detailArg(args.prune_error));
}

/**
 * The wording of one user action, from its code and arguments.
 *
 * The same shape as `detailText`: the core sends family.action and the raw
 * values, and the sentence belongs here. A code with no entry returns the code
 * itself rather than nothing — an id is ugly, but it is at least traceable, and
 * a blank row would look like data loss.
 */
export function activityText(action: string, args: Record<string, DetailArg> = {}): string {
  const dot = action.indexOf(".");
  if (dot < 0) return action;
  const families: Record<string, Record<string, string> | undefined> = messages.activity.codes;
  const copy = families[action.slice(0, dot)]?.[action.slice(dot + 1)];
  if (typeof copy !== "string") return action;
  // The wire format is untagged, so the argument's name is the only thing that
  // knows a number is a byte count — the same rule `hygieneArg` applies to a
  // finding. Formatting it in the core would freeze an English unit into the log.
  const text = (name: string): string => {
    const value = args[name];
    if (value === undefined) return "";
    if (typeof value === "number" && name.endsWith("_bytes")) return formatBytes(value);
    return detailArg(value);
  };
  return copy.replace(/\{(\w+)\}/g, (_, name: string) => text(name));
}

/**
 * The sentence for a failed scan, from its stable code.
 *
 * The raw Git or host error is deliberately not part of this mapping: the
 * operation log keeps it, and the reader gets a cause and a next step instead.
 */
export function scanFailureText(failure: ScanFailure): string {
  switch (failure.code) {
    case "empty":
      return messages.errors.scan.empty;
    case "not_found":
      return messages.errors.scan.notFound(failure.path);
    case "no_repositories":
      return messages.errors.noRepositories(failure.path);
    case "not_a_git_repository":
      return messages.errors.scan.notRepository(failure.path);
    case "git_unavailable":
      return messages.errors.scan.gitUnavailable;
    case "task_failed":
    case "unknown":
      return messages.errors.scan.unknown;
    default:
      return messages.errors.scan.unknown;
  }
}

/**
 * Sort comparator for lists ordered by size descending.
 *
 * A known size always comes before an unknown one: "unknown" is not a number and
 * cannot be called larger or smaller, so it stays at the end rather than being
 * treated as infinity. Equal and unknown sizes fall back to the caller's
 * tie-breaker.
 */
export function compareSizeNullsLast(a: number | null, b: number | null): number {
  if (a === null && b === null) return 0;
  if (a === null) return 1;
  if (b === null) return -1;
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
  const windows = isWindowsPath(path) || isWindowsPath(root);
  let key = normalizePathKey(path);
  let rootKey = normalizePathKey(root);
  if (rootKey.length === 0) return false;
  if (windows) {
    key = key.toLowerCase();
    rootKey = rootKey.toLowerCase();
  }
  return key === rootKey || key.startsWith(`${rootKey}/`);
}

/** Display-only abbreviation of a home directory; tolerates both separators. */
export function homePath(path: string): string {
  const home = /^(?:[A-Za-z]:)?[\\/](?:Users|home)[\\/][^\\/]+/i;
  const windows = /^[A-Za-z]:[\\/]/.test(path);
  return path.replace(home, windows ? "%USERPROFILE%" : "~");
}

export function basename(path: string): string {
  const parts = normalizePathKey(path).split("/").filter((part) => part !== "");
  return parts.length > 0 ? parts[parts.length - 1] : path;
}
