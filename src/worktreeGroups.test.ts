import { describe, expect, it } from "vitest";
import type { Reason, ReasonCode, WorktreeRecord } from "./types";
import { reviewLane } from "./worktreeGroups";

const root = "/Users/andy/worktrees/project-feature";

function reason(code: ReasonCode, severity: Reason["severity"] = "Review"): Reason {
  return { code, severity, text: code, evidence: "fixture" };
}

function worktree(
  codes: Array<Reason["code"]> = ["wt.unmerged"],
  overrides: Partial<WorktreeRecord> = {}
): WorktreeRecord {
  return {
    path: root,
    branch: "feature",
    head: "0123456789abcdef",
    is_main: false,
    is_detached: false,
    is_locked: false,
    is_prunable: false,
    dirty_files: 0,
    untracked_files: 0,
    untracked_file_list: [],
    ahead: 0,
    behind: 0,
    has_upstream: true,
    merged_into_default: false,
    absorbed_into_default: false,
    stash_count: 0,
    last_updated_at: null,
    agent: null,
    size_bytes: 1024,
    safety: {
      status: "Review",
      reasons: codes.map((code) => reason(code))
    },
    ...overrides
  };
}

describe("review worktree groups", () => {
  it("batches a clean worktree whose Review reasons are branch facts only", () => {
    expect(reviewLane(worktree(["wt.unpushed_branch", "wt.no_upstream", "wt.unmerged"]))).toBe(
      "batch"
    );
  });

  it("keeps untracked files in their own group", () => {
    expect(
      reviewLane(
        worktree(["wt.untracked", "wt.unmerged"], {
          untracked_files: 2,
          untracked_file_list: ["a.txt", "b.txt"]
        })
      )
    ).toBe("untracked");
  });

  it("keeps recent activity, detached HEAD, and unverifiable checks out of batch groups", () => {
    expect(reviewLane(worktree(["wt.unmerged", "wt.recent_activity"]))).toBe("attention");
    expect(reviewLane(worktree(["wt.detached"]))).toBe("attention");
    expect(reviewLane(worktree(["wt.cannot_verify"]))).toBe("attention");
    expect(
      reviewLane(
        worktree(["wt.untracked", "wt.recent_activity"], {
          untracked_files: 1,
          untracked_file_list: ["draft.txt"]
        })
      )
    ).toBe("attention");
  });

  it("does not batch an unknown Review reason", () => {
    const future = "wt.future" as unknown as ReasonCode;
    expect(
      reviewLane(
        worktree(["wt.unmerged"], {
          safety: { status: "Review", reasons: [reason(future)] }
        })
      )
    ).toBe("attention");
  });

  it("does not batch an inconsistent Review record", () => {
    expect(reviewLane(worktree(["wt.unmerged"], { merged_into_default: null }))).toBe("attention");
    expect(reviewLane(worktree(["wt.unmerged"], { dirty_files: 1 }))).toBe("attention");
    expect(reviewLane(worktree([]))).toBe("attention");
  });

  it("returns no lane for Safe and Unsafe records", () => {
    expect(
      reviewLane({
        ...worktree(),
        safety: { status: "Safe", reasons: [reason("wt.merged", "Safe")] }
      })
    ).toBeNull();
    expect(
      reviewLane({
        ...worktree(),
        safety: { status: "Unsafe", reasons: [reason("wt.dirty", "Unsafe")] }
      })
    ).toBeNull();
  });
});
