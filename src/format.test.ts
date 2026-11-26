import { describe, expect, it } from "vitest";
import {
  activityText,
  compareByActionability,
  compareSizeNullsFirst,
  detailText,
  formatBytes,
  formatCount,
  homePath,
  isUnder,
  pathIdentityKey,
  reasonText,
  scanFailureText
} from "./format";
import { applyLocale, messages, SOURCE_LOCALE } from "./i18n";
import { en } from "./i18n/en";
import { zhCN } from "./i18n/zh-CN";
import type { Reason, ReasonCode } from "./types";

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

describe("homePath", () => {
  it("shortens a home directory with the style the path uses", () => {
    expect(homePath("/Users/andy/MySrc/proj")).toBe("~/MySrc/proj");
    expect(homePath("/home/andy/proj")).toBe("~/proj");
    expect(homePath("C:\\Users\\andy\\MySrc\\proj")).toBe("%USERPROFILE%\\MySrc\\proj");
    expect(homePath("C:/Users/andy/MySrc/proj")).toBe("%USERPROFILE%/MySrc/proj");
  });

  it("leaves a path outside a home directory unchanged", () => {
    expect(homePath("C:\\Work\\repo")).toBe("C:\\Work\\repo");
    expect(homePath("/tmp/repo")).toBe("/tmp/repo");
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

describe("pathIdentityKey", () => {
  it("treats both separators and a trailing separator as one path", () => {
    const key = pathIdentityKey("C:\\Src\\repo", true);
    expect(pathIdentityKey("C:/Src/repo/", true)).toBe(key);
    expect(pathIdentityKey("C:\\Src\\repo\\", true)).toBe(key);
  });

  it("folds case only where the filesystem does", () => {
    // Windows: the same directory. macOS: two spellings that may be two
    // directories on a case-sensitive volume, so the caller decides.
    expect(pathIdentityKey("C:\\Src\\Repo", true)).toBe(pathIdentityKey("c:/src/repo", true));
    expect(pathIdentityKey("/Users/andy/Src/Repo", false)).not.toBe(
      pathIdentityKey("/Users/andy/src/repo", false)
    );
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

describe("activityText", () => {
  function inLocale(locale: "en" | "zh-CN", run: () => void): void {
    applyLocale(locale);
    try {
      run();
    } finally {
      applyLocale(SOURCE_LOCALE);
    }
  }

  it("writes a byte argument as a size in the reader's units", () => {
    // The plan line is what a reader goes back to when asking what a cleanup was
    // worth before it ran. A raw byte count there would make the one number that
    // matters unreadable, and formatting it in the core would freeze an English
    // unit into a log that outlives the language it was written in.
    const args = { worktrees: 3, artifacts: 6, total_bytes: 26_806_672_118 };
    for (const [locale, table] of [
      ["en", en],
      ["zh-CN", zhCN]
    ] as const) {
      inLocale(locale, () => {
        expect(activityText("plan.generated", args)).toBe(
          table.activity.codes.plan.generated
            .replace("{worktrees}", "3")
            .replace("{artifacts}", "6")
            .replace("{total_bytes}", formatBytes(26_806_672_118))
        );
      });
    }
    // Not the number itself: that is the fallback, not the sentence.
    expect(activityText("plan.generated", args)).not.toContain("26806672118");
  });

  it("keeps an action this build has no copy for traceable", () => {
    // A newer core against an older UI: an id is ugly, but a blank row reads as
    // data loss.
    expect(activityText("plan.something_new", {})).toBe("plan.something_new");
  });
});

describe("reasonText", () => {
  // Shaped like a real reason: an English sentence from the core plus the code
  // that says what the sentence means.
  const dirty: Reason = {
    code: "wt.dirty",
    severity: "Unsafe",
    text: "has uncommitted changes",
    evidence: "3 files have uncommitted changes"
  };

  function inLocale(locale: "en" | "zh-CN", run: () => void): void {
    applyLocale(locale);
    try {
      run();
    } finally {
      applyLocale(SOURCE_LOCALE);
    }
  }

  it("follows the active language rather than the core's sentence", () => {
    inLocale("en", () => {
      expect(reasonText(dirty)).toBe(en.reasons.wt.dirty);
    });
    inLocale("zh-CN", () => {
      // The point of the table: this is the sentence the reader gets, not the
      // English one that arrived with the record.
      expect(reasonText(dirty)).toBe(zhCN.reasons.wt.dirty);
      expect(reasonText(dirty)).not.toBe(dirty.text);
    });
  });

  it("keeps the core's sentence for a code this build does not know", () => {
    // A newer core against an older UI must still say something true rather than
    // an empty cell, so the fallback is the sentence and not a placeholder.
    const future: Reason = {
      ...dirty,
      code: "wt.brand_new_check" as ReasonCode,
      text: "a check this build has never heard of"
    };
    inLocale("zh-CN", () => {
      expect(reasonText(future)).toBe(future.text);
    });
  });

  it("covers the artifact family as well as the worktree one", () => {
    // Both families travel in the same `reasons` table; a lookup that only worked
    // for `wt.*` would fall back to English for every artifact row.
    inLocale("zh-CN", () => {
      const artifact: Reason = {
        code: "af.git_tracked",
        severity: "Unsafe",
        text: "the directory contains files committed to the repository",
        evidence: "git ls-files matches 2 files"
      };
      expect(reasonText(artifact)).toBe(zhCN.reasons.af.git_tracked);
    });
  });
});

describe("scanFailureText", () => {
  it("gives the not-a-repository cause and next step without the raw Git text", () => {
    const failure = {
      code: "not_a_git_repository" as const,
      path: "/tmp/plain-folder",
      detail: "git worktree list failed: fatal: not a git repository (or any parent directories)"
    };
    const text = scanFailureText(failure);
    expect(text).toBe(messages.errors.scan.notRepository("/tmp/plain-folder"));
    expect(text).not.toContain("fatal");
    expect(text).not.toContain("worktree list");
  });

  it("names a missing path and falls back to a generic scan sentence", () => {
    expect(scanFailureText({ code: "not_found", path: "/tmp/gone" })).toContain("/tmp/gone");
    expect(scanFailureText({ code: "unknown", detail: "fatal: unexpected" })).not.toContain(
      "fatal"
    );
  });
});

describe("per-item failure copy", () => {
  it("keeps Git's raw stderr out of the result sentence", () => {
    const text = detailText("worktree.remove_failed", {
      error: "fatal: '/repo/wt' contains modified or untracked files"
    });
    expect(text).not.toContain("fatal");
    expect(text).not.toContain("contains modified");
    expect(text).toBe(messages.detail.codes.worktree.remove_failed);
  });
});
