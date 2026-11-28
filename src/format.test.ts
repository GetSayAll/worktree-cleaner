import { describe, expect, it } from "vitest";
import {
  activityText,
  compareSizeNullsLast,
  detailText,
  formatBytes,
  formatCount,
  homePath,
  isUnder,
  normalizePathKey,
  pathIdentityKey,
  pathMatchesRule,
  reasonText,
  scanFailureText,
  warningText
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

describe("compareSizeNullsLast", () => {
  it("keeps an unmeasurable item after every measured one", () => {
    // Unknown is not a number and cannot be called larger; it stays last.
    expect(compareSizeNullsLast(null, 10)).toBe(1);
    expect(compareSizeNullsLast(10, null)).toBe(-1);
    expect(compareSizeNullsLast(null, null)).toBe(0);
  });

  it("sorts measured sizes descending", () => {
    // Negative means "a first": the larger number sorts earlier.
    expect(compareSizeNullsLast(300, 200)).toBe(-100);
    expect(compareSizeNullsLast(200, 300)).toBe(100);
    expect(compareSizeNullsLast(200, 200)).toBe(0);
  });

  it("orders a mixed list largest-first, with unknown sizes last", () => {
    const sizes = [50, null, 900, 10, null, 400];
    const sorted = [...sizes].sort(compareSizeNullsLast);
    expect(sorted).toEqual([900, 400, 50, 10, null, null]);
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

describe("normalizePathKey", () => {
  it("preserves the UNC prefix instead of turning it into a drive-relative path", () => {
    expect(normalizePathKey("\\\\server\\share\\repo")).toBe("//server/share/repo");
    expect(normalizePathKey("//server/share/repo/")).toBe("//server/share/repo");
  });

  it("still unifies separators and collapses repeated separators", () => {
    expect(normalizePathKey("C:\\Users\\andy\\repo")).toBe("C:/Users/andy/repo");
    expect(normalizePathKey("/Users/andy//repo/")).toBe("/Users/andy/repo");
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

  it("compares Windows paths case-insensitively", () => {
    expect(isUnder("C:\\Users\\Andy\\Repo\\dist", "c:\\users\\andy\\repo")).toBe(true);
    expect(pathMatchesRule("C:\\Users\\Andy\\Repo\\dist", "c:\\users\\andy\\repo")).toBe(true);
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

describe("warningText", () => {
  const rawWarnings: Array<[string, string]> = [
    ["Git reported no worktrees for /tmp/repo; its count is unknown", "Git 报告 /tmp/repo 没有工作树"],
    [
      "the worktree count for /tmp/repo could not be read: git worktree list exited with Some(128): fatal",
      "无法读取 /tmp/repo 的工作树数量"
    ],
    ["workspace discovery stopped after 250000 entries; some repositories may be missing", "工作区发现达到 250000 个条目上限"],
    ["workspace discovery stopped after 2000 repositories", "工作区发现达到 2000 个仓库上限"],
    ["/tmp/repo is inside the system Trash; discovery does not read the Trash", "位于废纸篓中"],
    [
      "found 5 other local clone(s) sharing the selected repository's origin remote under /tmp",
      "发现 5 个其他本地 clone"
    ],
    ["no Git repositories were found below /tmp", "没有找到 Git 仓库"],
    ["the repository stash count could not be read: git stash list exited with Some(1): err", "无法读取仓库的 stash 数量"],
    ["the scan budget of 10s ran out after 2 of 5 worktrees; the remaining ones were not scanned", "扫描预算 10 秒已用完"],
    ["size measurement of /tmp/x did not complete within 5s; the size is recorded as unknown", "大小测量在 5 秒内未完成"],
    ["3 worktree(s) inside the system Trash were not scanned; the Trash is left alone", "3 个工作树位于废纸篓中"],
    ["the scan budget ran out; artifact scanning stopped at /tmp/x (4 directories checked so far)", "构建产物扫描停在 /tmp/x"],
    ["/tmp/x is no longer on disk; it is reported as unmeasurable rather than empty", "已不在磁盘上"],
    ["/tmp/x could not be read while looking for build caches: denied", "查找构建缓存时无法读取 /tmp/x"],
    ["the build-cache scan stopped after 200000 entries; some caches may be missing", "构建缓存扫描达到 200000 个条目上限"],
    ["the build-cache root does not exist or is not a directory: /tmp", "构建缓存根目录不存在"],
    ["no directories carrying CACHEDIR.TAG were found below /tmp", "带 CACHEDIR.TAG 的目录"],
    ["/tmp/x could not be inspected: denied", "无法检查 /tmp/x"],
    ["size measurement of /tmp/x did not complete; the cache is shown as unmeasurable", "缓存显示为不可测量"],
    ["the ignore check could not answer for /x: exited with Some(128): err", "无法为 /x 完成 ignore 检查"],
    ["the scan budget ran out; npm-cache and the slots after it were not measured", "npm-cache 及其后面的槽位未测量"],
    [".worktree-keep could not be read, so its exceptions do not apply: denied", ".worktree-keep 无法读取"]
  ];

  function inLocale(locale: "en" | "zh-CN", run: () => void): void {
    applyLocale(locale);
    try {
      run();
    } finally {
      applyLocale(SOURCE_LOCALE);
    }
  }

  it("translates every known scanner warning and keeps technical detail", () => {
    inLocale("zh-CN", () => {
      for (const [raw, fragment] of rawWarnings) {
        const text = warningText(raw);
        expect(text, raw).toContain(fragment);
      }
      expect(warningText(rawWarnings[1][0])).toContain("/tmp/repo");
      expect(warningText(rawWarnings[1][0])).toContain("Some(128)");
    });
  });

  it("keeps the source warning in English unchanged", () => {
    inLocale("en", () => {
      for (const [raw] of rawWarnings) {
        if (raw.includes("Trash")) {
          expect(warningText(raw)).toContain("Trash");
          continue;
        }
        expect(warningText(raw)).toBe(raw);
      }
    });
  });

  it("leaves an unknown warning traceable rather than blank", () => {
    const future = "a newer core added a note this build does not know";
    expect(warningText(future)).toBe(future);
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
