import { messages } from "./i18n";
import type { DetailArg, DetailCode, HygieneFinding, Reason, ReasonCode } from "./types";

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
    .replace("{pruneError}", detailArg(args.prune_error))
    .replace("{error}", detailArg(args.error));
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
  const text = (value: DetailArg | undefined): string =>
    value === undefined ? "" : detailArg(value);
  return copy.replace(/\{(\w+)\}/g, (_, name: string) => text(args[name]));
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
