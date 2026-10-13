import { describe, expect, it } from "vitest";
import {
  compareByActionability,
  compareSizeNullsFirst,
  formatBytes,
  formatCount,
  isUnder
} from "./format";
import { messages } from "./i18n";

// `size_bytes` is `number | null` since schema version 2: an item whose size
// could not be measured must never be rendered as `0 B`, and a total that omits
// it must be shown as a lower bound. These tests pin that contract down.

describe("formatBytes", () => {
  it("answers unknown for a size that could not be measured", () => {
    // Not "0 B": zero is a measurement, null is the absence of one.
    expect(formatBytes(null)).toBe(messages.common.unknown);
    // The approximate flag cannot rescue a null either: there is no number to
    // prefix, so the answer stays "unknown" with no stray "≥".
    expect(formatBytes(null, { approximate: true })).toBe(messages.common.unknown);
  });

  it("prefixes a lower bound only when asked", () => {
    expect(formatBytes(2048, { approximate: true })).toBe(`≥ 2.00 ${messages.units.kilobytes}`);
    expect(formatBytes(2048)).toBe(`2.00 ${messages.units.kilobytes}`);
    // A byte-level value takes the prefix too.
    expect(formatBytes(512, { approximate: true })).toBe(`≥ 512 ${messages.units.bytes}`);
  });

  it("switches units at the binary boundaries", () => {
    expect(formatBytes(0)).toBe(`0 ${messages.units.bytes}`);
    expect(formatBytes(1023)).toBe(`1023 ${messages.units.bytes}`);
    expect(formatBytes(1024)).toBe(`1.00 ${messages.units.kilobytes}`);
    expect(formatBytes(1024 * 1024)).toBe(`1.00 ${messages.units.megabytes}`);
    expect(formatBytes(1024 * 1024 * 1024)).toBe(`1.00 ${messages.units.gigabytes}`);
    expect(formatBytes(1024 ** 4)).toBe(`1.00 ${messages.units.terabytes}`);
  });

  it("caps at terabytes instead of inventing a unit", () => {
    // A petabyte-sized directory must stay in TB with a readable number.
    expect(formatBytes(1024 ** 5)).toBe(`1024 ${messages.units.terabytes}`);
  });

  it("drops decimals as the number grows", () => {
    expect(formatBytes(5 * 1024)).toBe(`5.00 ${messages.units.kilobytes}`);
    expect(formatBytes(10 * 1024)).toBe(`10.0 ${messages.units.kilobytes}`);
    expect(formatBytes(100 * 1024)).toBe(`100 ${messages.units.kilobytes}`);
  });
});

describe("compareSizeNullsFirst", () => {
  it("puts an unmeasurable item above every measured one", () => {
    // Burying the item the user cannot assess at the bottom of a "largest" list
    // would hide exactly the row that most needs attention.
    expect(compareSizeNullsFirst(null, 10)).toBe(-1);
    expect(compareSizeNullsFirst(10, null)).toBe(1);
    expect(compareSizeNullsFirst(null, null)).toBe(0);
  });

  it("sorts measured sizes descending", () => {
    // Negative means "a first": the larger number sorts earlier.
    expect(compareSizeNullsFirst(300, 200)).toBe(-100);
    expect(compareSizeNullsFirst(200, 300)).toBe(100);
    expect(compareSizeNullsFirst(200, 200)).toBe(0);
  });

  it("orders a mixed list unknown-first, then largest-first", () => {
    const sizes = [50, null, 900, 10, null, 400];
    const sorted = [...sizes].sort(compareSizeNullsFirst);
    expect(sorted).toEqual([null, null, 900, 400, 50, 10]);
  });
});

describe("formatCount", () => {
  it("groups thousands for the file-count columns", () => {
    expect(formatCount(18240)).toBe("18,240");
    expect(formatCount(0)).toBe("0");
  });
});

describe("isUnder", () => {
  // Used to tell the reader that a worktree of the repository they picked lives
  // somewhere else. `git worktree add` takes any path, so a sibling directory, a
  // shared `~/worktrees/…` or a scratch directory in `/tmp` are all ordinary.
  const root = "/Users/andy/MySrc/proj";

  it("accepts the root itself and anything below it", () => {
    expect(isUnder(root, root)).toBe(true);
    expect(isUnder(`${root}/dist`, root)).toBe(true);
    expect(isUnder(`${root}/node_modules/left-pad`, root)).toBe(true);
  });

  it("rejects a path outside it, including one that only shares a prefix", () => {
    expect(isUnder("/Users/andy/MySrc/proj--feature", root)).toBe(false);
    expect(isUnder("/Users/andy/MySrc", root)).toBe(false);
    expect(isUnder("/tmp/scratch/wt", root)).toBe(false);
    expect(isUnder("/Users/andy/worktrees/proj/feature", root)).toBe(false);
  });

  it("does not match everything when the root is empty", () => {
    // A snapshot with no root must not make every path look like it is inside one.
    expect(isUnder(root, "")).toBe(false);
  });
});

describe("compareByActionability", () => {
  // The list order answers "where do I start". Keeping the scan's own order
  // scattered the removable items among the main worktree and the dirty ones, so
  // finding them meant reading every row.
  const rec = (status: string) => ({ safety: { status } });

  it("puts the safe ones first, then the ones needing a decision, then the rest", () => {
    const order = ["Unsafe", "Safe", "Review"].map(rec).sort(compareByActionability);
    expect(order.map((r) => r.safety.status)).toEqual(["Safe", "Review", "Unsafe"]);
  });

  it("sorts anything it does not recognise to the end, never to the front", () => {
    // An unknown level is a level this build does not know about. Putting it
    // first would offer it as the obvious thing to act on; putting it last keeps
    // it out of the way until someone reads it.
    const order = ["?weird", "Safe"].map(rec).sort(compareByActionability);
    expect(order.map((r) => r.safety.status)).toEqual(["Safe", "?weird"]);
  });

  it("leaves items of the same level in step with the caller", () => {
    // Zero, deliberately: the volume ordering is a second pass, composed by the
    // caller. A non-zero here would fight it.
    expect(compareByActionability(rec("Safe"), rec("Safe"))).toBe(0);
  });
});
