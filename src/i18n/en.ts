// English message table — the source locale for Worktree Cleaner.
//
// Everything a user can read lives here, grouped by area and nested at most
// three levels deep. Interpolated values are arrow functions rather than
// template placeholders so a translation is free to reorder or drop arguments
// to fit its own grammar.
//
// This file holds user-visible copy only. Scanner-produced reason text and
// evidence are data, not UI chrome, and stay with the scan results in
// src/mock/snapshot.ts.

// Kept as a module constant so the plan blocker copy and the code that strips
// that prefix back off stay in sync.
const blockerPrefix = "Blocked by whitelist protection: ";

export const en = {
  // The words that differ by the machine the app runs on. A string writes
  // `{{trash}}` and `applyLocale` fills it in (see `./index.ts`), so the interface
  // names the one system the reader is on. `other` is a browser preview on a third
  // system: the generic noun rather than a guess.
  platform: {
    trash: { macos: "Trash", windows: "Recycle Bin", other: "system trash" }
  },
  app: {
    name: "Worktree Cleaner",
    // Intentionally blank until the tagline is decided. The key stays so that
    // landing the copy later is a one-line edit and nothing else has to move.
    tagline: "",
    readOnlyNote:
      "The scan phase runs only read-only Git commands; deletions always go to the system {{trash}} and are never permanent.",
    versionNote: "v0.1.0",
    sampleData: "Sample data",
    previewNote: "Browser preview",
    switchProject: "Switch project",
    noProject: "No project selected",
    pickRepository: "Pick a repository to start scanning",
    desktopOnly:
      "This action needs the desktop app; the browser preview cannot reach the file system."
  },
  preview: {
    // Said on the pages that offer an action, because a button that looks available
    // and does nothing is the failure this build was rewritten to remove.
    notice:
      "Browser preview — this build cannot reach the file system, so the cleanup actions are disabled and the numbers on these pages come from sample data."
  },
  nav: {
    project: "Project",
    scan: "Scan",
    overview: "Overview",
    worktrees: "Worktree",
    artifacts: "Build artifacts",
    plan: "Cleanup plan",
    result: "Result",
    history: "History",
    settings: "Settings"
  },
  status: {
    label: {
      Safe: "Safe to delete",
      Review: "Needs review",
      Unsafe: "Must not delete"
    },
    short: {
      Safe: "Safe",
      Review: "Review",
      Unsafe: "Blocked"
    }
  },
  // One entry per ReasonCode, in the order `src/types.ts` declares them. The core
  // ships the same codes, so a verdict can be rendered in the reader's language
  // instead of the language of the process that found it. `evidence` is not here:
  // it is a command, a count or a path, and it stays verbatim.
  //
  // `wt.cannot_verify` has four wordings in the core ("cannot read the worktree
  // status", "cannot read the HEAD commit time", "cannot read the merge status",
  // "cannot read {label}"). The conclusion is identical in all four, so one entry
  // covers them and the evidence line below carries the detail.
  reasons: {
    wt: {
      main: "this is the main worktree",
      locked: "locked by Git",
      dirty: "has uncommitted changes",
      unpushed: "has commits that were never pushed",
      detached: "HEAD is detached",
      no_upstream: "has no upstream branch and is not merged",
      unmerged: "not merged into the default branch",
      untracked: "has untracked files",
      stash: "has stash entries",
      recent_activity: "changed within the activity window",
      agent_active: "a Git command may be running",
      cannot_verify: "cannot complete the check",
      whitelisted: "matches a whitelist rule",
      prunable: "the working directory no longer exists",
      merged: "merged into the default branch",
      clean: "clean working tree"
    },
    af: {
      git_tracked: "the directory contains files committed to the repository",
      nested_git: "contains a nested Git repository",
      authored_content: "contains hand-written content",
      credentials: "contains what looks like a credential file",
      symlink_outside: "symlink pointing outside the project",
      whitelisted: "protected by a whitelist rule",
      recent_activity: "changed recently",
      cannot_verify: "could not complete the check",
      empty: "the directory is empty",
      git_ignored: "ignored by .gitignore",
      rebuildable: "regenerable directory"
    }
  },
  // What one executed action did, keyed by DetailCode (src/types.ts). The core
  // decides the code and the raw values; the sentence is written here, which is
  // why the detail column follows the language instead of staying English.
  //
  // `{before}` and `{after}` are the two sides of a field that moved, `{reason}`
  // is a reason code that the reason table below also knows, and `{error}` is
  // whatever Git or the system said. `{field}` is looked up in `fields` and
  // removed before this runs.
  detail: {
    yes: "yes",
    no: "no",
    // Keyed as family/name, not as the dotted code, for the same reason
    // `reasons` is: the test helpers walk a table by splitting a path on dots,
    // so a key that contains one would be read as two segments.
    codes: {
      worktree: {
        not_in_scan: "no longer in the scan result; nothing was executed",
        changed: "changed since the plan was made: {before} → {after}; not deleted",
        no_longer_safe: "the re-check no longer reports it as safe to delete: {reason}",
        removed: "removed with git worktree remove",
        remove_failed: "git worktree remove failed: {error}",
        pruned: "git worktree prune cleaned up the leftover metadata",
        prune_failed: "git worktree prune failed: {error}"
      },
      artifact: {
        not_in_scan: "no longer in the scan result; nothing was executed",
        not_a_known_kind:
          "no longer a directory name this tool recognises as an artifact; not deleted",
        changed: "changed since the plan was made: {before} → {after}; not deleted",
        no_longer_safe: "the re-check no longer reports it as safe to delete: {reason}",
        vanished: "this directory is no longer on disk, so there was nothing to delete",
        unmeasurable: "the re-check could not measure this directory, so it was left alone",
        trashed: "moved to the {{trash}}",
        trash_failed: "could not move to the {{trash}}: {error}"
      },
      prunable: {
        metadata_only:
          "the directory was already gone; only the Git metadata was cleaned up, and no files were touched"
      },
      precondition: {
        failed: "a deletion guard refused this path; not executed"
      },
      bundled: {
        with_worktree: "removed together with its own worktree; no separate operation"
      }
    },
    // The name of the field that moved, for the `changed` codes. Without these
    // the sentence would read "size changed … " with the field name in English.
    fields: {
      level: "the verdict",
      prunable: "the working directory",
      dirty: "the uncommitted changes",
      untracked: "the untracked files",
      merged: "the merge status",
      size: "the size",
      files: "the file count",
      tracked: "Git tracking",
      ignored: "the ignore rules",
      symlink: "the symlink"
    },
    legacy: "recorded before this build kept reason codes"
  },
  // What the user did, keyed by ActivityAction. The deletions live in their own
  // section above; these are the other half of "what happened on this machine",
  // and they are the half a reader has no other way to reconstruct.
  activity: {
    codes: {
      scan: {
        started: "Started a scan",
        finished: "Scan finished: {worktrees} worktrees, {artifacts} artifact directories",
        failed: "The scan did not finish: {error}",
        cancelled: "Cancelled the scan after {elapsed}"
      },
      project: {
        switched: "Switched to this project from {from}"
      },
      plan: {
        generated: "Generated a plan: {worktrees} worktrees, {artifacts} artifact directories",
        exported: "Exported the plan"
      },
      settings: {
        changed: "Changed a setting"
      },
      cleanup: {
        confirmed:
          "Confirmed the cleanup: {worktrees} worktrees, {artifacts} artifact directories, {forced} needing --force"
      },
      trash: {
        opened: "Opened the system {{trash}}"
      }
    },
    kinds: {
      deletion: "Deleted",
      action: "You did"
    },
    filterAction: "Your actions"
  },
  common: {
    unknown: "unknown",
    none: "(none)",
    emDash: "—",
    detached: "(detached)",
    detachedHead: "(detached HEAD)",
    unknownAgent: "unknown agent",
    defaultBranch: "default branch",
    unsafeTooltip: "Must not delete items cannot be selected",
    mainWorktree: (name: string) => `${name} (main worktree)`,
    rescan: "Rescan",
    copied: "Copied",
    action: {
      worktreeRemove: "Remove Worktree",
      trashMove: "Move to {{trash}}",
      prune: "Clean up metadata"
    },
    outcome: {
      // Every row on the result page is a deletion, so the label says what happened
      // to the thing rather than naming the run's internal state.
      ok: "Deleted",
      skipped: "Not deleted",
      failed: "Delete failed"
    }
  },
  units: {
    bytes: "B",
    kilobytes: "KB",
    megabytes: "MB",
    gigabytes: "GB",
    terabytes: "TB",
    atLeast: "≥ ",
    seconds: (n: number) => `${n}s`,
    fileCount: (count: string) => `${count} files`
  },
  time: {
    justNow: "just now",
    hoursAgo: (n: number) => `${n} hours ago`,
    oneDayAgo: "1 day ago",
    daysAgo: (n: number) => `${n} days ago`,
    monthsAgo: (n: number) => `${n} months ago`,
    yearsAgo: (n: number) => `${n} years ago`
  },
  scan: {
    title: "Scanning",
    elapsed: (clock: string) => `Elapsed ${clock}`,
    cancel: "Cancel",
    scanning: "scanning…",
    budgetNote:
      'Each directory probe has a timeout and the whole scan has a total budget; if the budget runs out it reports "scan incomplete" and marks the affected items as "Needs review".',
    stages: {
      worktrees: {
        label: "Parse Git worktrees",
        detail: "git worktree list --porcelain -z"
      },
      status: {
        label: "Check each worktree's status",
        detail: "status · merge-base · stash · upstream"
      },
      artifacts: {
        label: "Scan build artifacts",
        detail: "walk a list of artifact kinds within project boundaries"
      },
      verify: {
        label: "Verify and summarize",
        detail: "activity re-check · reason merge · size rollup"
      }
    }
  },
  welcome: {
    eyebrow: "Worktree Cleaner",
    title: "Project cleanup audit",
    lede: "Scan a repository, list all of its Git worktrees and build artifacts, and explain item by item why each one can or cannot be deleted. Scanning is read-only by default; deletions always go to the system {{trash}}.",
    pathPlaceholder: "/path/to/your-repo",
    chooseFolder: "Choose folder",
    chooseFolderTitle: "Open the system folder picker",
    chooseFolderUnavailable:
      "The folder picker lives in the desktop app; type the path instead",
    pickerFailed: (message: string) => `The folder picker could not open: ${message}`,
    startScan: "Start scan",
    dropHint: "You can also drop a project folder onto this window. The scan phase writes no files.",
    scanFailed: (message: string) => `The scan did not run: ${message}`,
    promises: {
      readOnlyTitle: "Read-only scan",
      readOnlyBody:
        "The check phase runs only read-only Git commands; it does not modify the repository or write into the target directory.",
      evidenceTitle: "Verifiable reasons",
      evidenceBody:
        'Every judgment comes with evidence: a concrete command, file count, or path. "Looks like junk" is not accepted.',
      trashTitle: "Deletion through the {{trash}}",
      trashBody:
        "Directory deletions always go to the system {{trash}}; worktrees go through git worktree remove. Nothing is permanently deleted."
    },
    recentTitle: "Recent projects",
    recentHint: "Click to scan",
    lastScan: (when: string) => `Last scan ${when}`,
    neverScanned: "not scanned yet",
    recentEmpty: "No projects yet; pick one to start"
  },
  overview: {
    title: "Overview",
    subtitle: (root: string, branch: string) => `${root} · default branch ${branch}`,
    viewWorktrees: "View Worktree",
    viewArtifacts: "View build artifacts",
    stats: {
      safeLabel: "Safe to delete",
      safeSub: (bytes: string) => `Total ${bytes} · Safe items are selected by default`,
      // Some Safe items timed out during measurement: their bytes are missing, so
      // the total is a lower bound and the count of unmeasurable items is stated
      // rather than silently dropped. The `bytes` argument already carries the
      // "≥" prefix (formatBytes with approximate), so this text must not repeat it.
      safeSubUnknown: (bytes: string, unknown: number) =>
        `${bytes} total · ${unknown} unmeasurable · Safe items are selected by default`,
      reviewLabel: "Needs review",
      reviewSub: "not merged / recently changed / check incomplete",
      unsafeLabel: "Must not delete",
      unsafeSub: "main worktree / dirty / locked / committed to the repository",
      reclaimableLabel: "Estimated reclaimable",
      reclaimableSub: (count: number, worktrees: number, artifacts: number) =>
        `Selected ${count} items · Worktree ${worktrees} · artifacts ${artifacts}`
    },
    warningTitle: "Scan incomplete · ",
    largestWorktrees: "Largest worktrees",
    viewAll: (n: number) => `View all ${n}`,
    mainWorktreeSuffix: " · main worktree",
    largestArtifacts: "Largest build artifacts",
    artifactMeta: (ecosystem: string, files: string) => `${ecosystem} · ${files} files`,
    selected: (n: number) => `Selected ${n} items · Estimated reclaimable`,
    // Under the reclaimable card: an unmeasurable selection makes the figure a
    // lower bound, and the user must be able to see that at a glance.
    selectedUnknown: (n: number) =>
      `${n} selected items have no measurable size, so the total shown is a lower bound`,
    selectHint: "Select items on the Worktree or Build artifacts page; Safe items are selected by default",
    adjustSelection: "Adjust selection",
    generatePlan: "Generate cleanup plan"
  },
  worktrees: {
    title: "Worktree",
    dataSource:
      "Data source: git worktree list --porcelain -z · status --porcelain=v2 · merge-base --is-ancestor",
    // `git worktree add` takes any path, so a worktree of the repository the user
    // picked can sit anywhere. The page says so rather than leaving the reader to
    // notice that a path is not under the directory at the top of the window.
    outsideRoot: "outside the selected directory",
    outsideRootHint:
      "This worktree sits outside the directory you picked. It is scanned, judged and can be removed like any other worktree.",
    filterAll: (n: number) => `All ${n}`,
    filterSafe: (n: number) => `Safe to delete ${n}`,
    filterReview: (n: number) => `Needs review ${n}`,
    filterUnsafe: (n: number) => `Must not delete ${n}`,
    searchPlaceholder: "Search by path or branch",
    selectAll: "Select every worktree that can be removed",
    details: "Details",
    collapse: "Collapse",
    gitFacts: "Git facts",
    facts: {
      fullPath: "Full path",
      head: "HEAD",
      upstream: "Upstream",
      mergeStatus: "Merge status",
      stash: "stash",
      untrackedFiles: "Untracked files",
      directorySize: "Directory size"
    },
    stashEntries: (n: number) => `${n} entries`,
    prunableBanner:
      "The working directory no longer exists; only git worktree prune runs to clean up leftover metadata. No files are touched.",
    untrackedLabel: "Untracked files (removing this worktree requires --force)",
    empty: "No worktrees match the current filters",
    upstreamGone: "working directory is gone",
    upstreamNone: "no upstream branch",
    upstreamCounts: (ahead: number, behind: number) => `upstream ahead ${ahead} / behind ${behind}`,
    mergeMain: "main worktree; not evaluated for merge status",
    mergeUnknown: (branch: string) => `cannot be determined whether merged into ${branch}`,
    mergeYes: (branch: string) => `merged into ${branch}`,
    mergeNo: (branch: string) => `not merged into ${branch}`,
    stateGone: "directory is gone",
    stateDirty: (n: number) => `dirty ${n}`,
    stateClean: "clean",
    stateUntracked: (n: number) => `untracked ${n}`,
    stateLocked: "locked",
    selectedSummary: (worktrees: number, artifacts: number) =>
      `Selected ${worktrees} worktrees · ${artifacts} artifacts · Estimated reclaimable`,
    continueToArtifacts: "Continue to artifacts",
    generatePlan: "Generate cleanup plan"
  },
  artifacts: {
    title: "Build artifacts",
    caption:
      "Candidate directory names are only hints; the final call is based on whether it is Git-tracked, whether it contains authored content, and its activity",
    bySize: "By size",
    byLastActivity: "By last activity",
    showOnlyLargerThan: "Show only larger than",
    summary: (total: number, safe: number, review: number, blocked: number) =>
      `${total} total · Safe ${safe} · Review ${review} · Blocked ${blocked}`,
    groupMeta: (items: number, bytes: string) => `${items} items · ${bytes}`,
    selectAllSafe: "Select all safe items",
    bundledTitle: "Removed together with its owning worktree",
    bundledBadge: "Removed with worktree",
    unowned: "(not owned by any worktree)",
    worktreeSuffix: (name: string) => `${name} (worktree)`,
    fileCountUnknown: "file count unknown",
    symlink: (target: string) => ` · symlink → ${target}`,
    gitTracked: "tracked",
    gitIgnored: "ignored",
    gitUnverified: "unverified",
    gitUntracked: "untracked",
    // The rows name their own fields, so there is no column header to label.
    // This prefix is what stands in for it: "Git ignored" rather than "ignored".
    gitLabel: "Git",
    empty: "No artifact directories exceed the current size threshold",
    selectedSummary: (artifacts: number, worktrees: number) =>
      `Selected ${artifacts} artifacts · ${worktrees} worktrees · Estimated reclaimable`,
    backToWorktrees: "Back to Worktree",
    generatePlan: "Generate cleanup plan"
  },
  plan: {
    title: "Cleanup plan",
    lede: "Review each item before executing; this page only describes what will happen — nothing has been executed yet.",
    backToAdjustSelection: "Back to adjust selection",
    stats: {
      removeWorktree: "Remove Worktree",
      removeWorktreeSub: "git worktree remove",
      moveToTrash: "Move to {{trash}}",
      moveToTrashSub: (bundled: number) => `plus ${bundled} items removed with their worktree`,
      reclaimable: "Estimated reclaimable",
      reclaimableSub: "based on sizes measured at scan time",
      // The total omits items whose size could not be measured, so it is a lower
      // bound; the count keeps that visible instead of implying an exact figure.
      reclaimableSubUnknown: (n: number) =>
        `At least the measured sizes · ${n} items have no measurable size`,
      autoExcluded: "Auto-excluded",
      autoExcludedSub: "Must not delete items are excluded from the plan"
    },
    blockerPrefix,
    blocker: (path: string) => `${blockerPrefix}${path}`,
    blockerTitle: "Precondition failed · ",
    ack: {
      force: (n: number) =>
        `Includes ${n} worktrees that need --force; their untracked files will be deleted too`,
      // The thresholds are arguments rather than literals: the plan code owns
      // them (ACK_COUNT_THRESHOLD / ACK_SIZE_THRESHOLD_BYTES) and passes them in,
      // so the numbers a reader sees cannot drift from the numbers the code
      // actually enforces. The size limit arrives already formatted, so the unit
      // follows the active language.
      count: (n: number, threshold: number) =>
        `This run will process ${n} items, above the ${threshold}-item confirmation threshold`,
      size: (limit: string) => `A single run frees more than ${limit}`,
      // With unmeasurable items in the plan the total is a lower bound, so the
      // size threshold above cannot be evaluated: this reason gates the run on
      // its own instead of guessing.
      unmeasurable: (n: number, limit: string) =>
        `Includes ${n} items whose size could not be measured, so the total is a lower bound and the ${limit} threshold cannot be evaluated`
    },
    checks: {
      title: "Precondition checks (dry run)",
      note: "Checks marked with a dash run again just before each deletion",
      pathCheck: (n: number) =>
        `Path check passed: non-empty absolute paths, symlinks resolved, no .. components, none pointing at system directories (${n} items)`,
      whitelistPassed: "Whitelist check passed: no item matches a protection rule",
      whitelistBlocked: (n: number) => `${n} items matched the whitelist and have been blocked`,
      duplicateCheck: (n: number) =>
        `Duplicate-deletion check passed: ${n} artifacts are removed with their owning worktree and counted only once`,
      // Each deferred check now says only what it will look at. Saying "has not
      // been performed yet" here repeated the dash in the list and the timing in
      // `note` a third time, and the re-check is an implementation, not a thing a
      // reader can act on.
      identityNotRun: "Re-check of path, size and last-modified time",
      activityNotRun: "Whether anything was written since this plan was generated"
    },
    channelA: "Channel A · Remove worktrees",
    items: (n: number) => `${n} items`,
    noWorktrees: "No worktrees selected.",
    needForce: "Needs --force",
    removable: "Removable directly",
    forceNote: (n: number) =>
      `--force also deletes the following untracked files (${n} in total), which are not tracked in Git and cannot be recovered from the repository once deleted:`,
    prunableNote:
      "The working directory is already gone. Only the Git metadata is pruned; no files are touched.",
    cleanNote: "The working tree is clean; no unsaved content will be lost.",
    channelB: "Channel B · Move to system {{trash}}",
    trashHeader: (n: number, bytes: string) => `${n} items · ${bytes}`,
    columns: {
      directory: "Directory",
      files: "Files",
      size: "Size",
      recoverability: "Recoverability"
    },
    removedWithWorktree: "Removed with worktree",
    recoverable: "Recoverable from {{trash}}",
    noArtifacts: "No artifact directories selected",
    confirmationTitle: "Confirmation before execution",
    confirmationIntro:
      "This operation meets the following high-risk conditions and requires manual confirmation:",
    ackConfirm: (n: number) =>
      `I have reviewed the items listed above, including the untracked files force-deleted in ${n} worktrees`,
    typeNameBefore: "Type the project directory name",
    typeNameAfter: "to proceed",
    exportTitle: "Plan export (JSON)",
    copy: "Copy",
    hideExport: "Hide export",
    exportPlan: "Export plan",
    excludedNote: (n: number) =>
      `${n} must-not-delete items were auto-excluded; return to the list to see why`,
    backToAdjust: "Back to adjust",
    runCleanup: "Run cleanup",
    running: "Running…",
    runningNote:
      "Re-checking and removing item by item. Every item is verified again first, so it takes a moment — please leave the app open.",
  },
  result: {
    title: "Result",
    openTrash: "Open {{trash}}",
    trashNotice:
      "Directories were moved to the system {{trash}} and are recoverable from there; worktrees were removed with git.",
    openLog: "Open log",
    logFailedTitle: "The operation log could not be written",
    logFailedBody: (path: string, error: string) =>
      `The items above were processed anyway, but ${path} could not be written (${error}), so the audit trail has a hole in it.`,
    stats: {
      spaceFreed: "Space freed",
      spaceFreedSub: "includes {{trash}} moves and worktrees",
      // Items that could never be measured contribute nothing to `freed`; saying
      // "at least" keeps the number honest instead of reporting a false total.
      spaceFreedSubUnknown: (n: number) =>
        `At least the measured sizes · ${n} items had no measurable size`,
      // A count that mixes worktrees and directories cannot be checked against the
      // Trash: `git worktree remove` deletes in place and never goes through it.
      // Saying so here is what stops "4 succeeded, 1 in the Trash" looking wrong.
      succeeded: "Handled",
      succeededSub: (trashed: number) =>
        `${trashed} went to the {{trash}}; worktrees are removed by git and do not go there`,
      movedToTrash: "Went to {{trash}}",
      movedToTrashSub: "recoverable from {{trash}}",
      skippedFailed: "Not deleted",
      skippedFailedSub: "nothing was removed; the table below says why"
    },
    perItemTitle: "Per-item results",
    columns: {
      action: "Action",
      target: "Target",
      detail: "Detail",
      result: "Result",
      size: "Size"
    },
    empty: "No execution records",
    footer:
      "Failed items do not affect the others; every action is written to the local operation log. Deletions are not permanent, and directory targets can be recovered from {{trash}}.",
    complete: "Cleanup complete; rescan to confirm the result",
    viewHistory: "View history",
    backToOverview: "Back to overview"
  },
  history: {
    title: "History",
    lede: "Logs are stored locally and never uploaded. See Settings for the location.",
    copyLog: "Copy log",
    filterAll: (n: number) => `All ${n}`,
    filterWorktreeRemove: "Remove Worktree",
    filterTrashMove: "Move to {{trash}}",
    empty: "No records",
    readFailed: (message: string) => `The log could not be read: ${message}`,
    reason: {
      confirmed: "Executed after user confirmation",
      rebuildable: "Matched a rebuildable artifact type",
      metadata: "Metadata cleanup"
    }
  },
  settings: {
    title: "Settings",
    currentProject: "Current project:",
    scanScopeTitle: "Scan scope",
    extraScanPaths: "Extra scan paths",
    extraScanPathsHint:
      "One per line; when empty, the defaults are used (~/MySrc, ~/dev, ~/Projects, ~/Code)",
    maxDepth: "Maximum scan depth",
    maxDepthHint: "Relative to the scan root; deeper is slower",
    perDirTimeout: "Per-directory timeout",
    perDirTimeoutHint:
      "After a timeout the item is treated as Needs review and never as Safe to delete",
    totalBudget: "Total scan budget",
    totalBudgetHint: "Stops when the budget runs out and honestly reports the unscanned portion",
    thresholdsTitle: "Thresholds",
    activityWindow: "Activity window (days)",
    activityWindowHint:
      "An item is only considered quiet if it has had no writes for longer than this; otherwise it is marked Needs review",
    artifactThreshold: "Artifact display size threshold",
    artifactThresholdHint: "Artifacts smaller than this are hidden by default to reduce noise",
    untrackedUnsafe: "Treat untracked files as unsafe",
    untrackedUnsafeHint:
      "When enabled, a worktree containing untracked files is marked Must not delete",
    protectionTitle: "Protection rules",
    whitelistPaths: "Whitelist paths",
    whitelistPathsHint: "One per line; matching directories never enter the cleanup plan",
    protectedBranches: "Protected branches",
    protectedBranchesHint:
      "Comma-separated, wildcards supported; the default branch is detected and added automatically",
    deletionTitle: "Deletion method",
    directoryDeletion: "Directory deletion",
    directoryDeletionHint:
      "Fixed to the system {{trash}}; this tool provides no permanent deletion and no bypass",
    directoryDeletionOption: "System {{trash}}",
    worktreeDeletion: "Worktree deletion",
    worktreeDeletionHint:
      "Always via git worktree remove; uses git worktree prune when the directory is gone",
    worktreeDeletionOption: "git worktree remove (never rm -rf)",
    strongProtection: "Strong protection",
    strongProtectionHint:
      "When enabled, the project directory name must be typed by hand before cleanup",
    logsTitle: "Logs, language and appearance",
    operationLog: "Operation log",
    operationLogHint: (n: number) => `Written locally and never uploaded. ${n} records so far.`,
    operationLogUnknown: "This platform has no log directory to report.",
    openDirectory: "Open directory",
    clearLog: "Clear log",
    clearLogTitle: "Empty the log file; the file itself stays",
    language: "Language",
    languageHint: "Follows the system by default and can be pinned to one language",
    locale: {
      system: "Follow system",
      en: "English",
      zhCN: "简体中文"
    },
    appearance: "Appearance",
    appearanceHint:
      '"Follow system" follows the light and dark modes of the operating system',
    system: "System",
    light: "Light",
    dark: "Dark",
    pathAbbrevBefore: "Paths are shown using abbreviations like",
    pathAbbrevAfter: "; the full absolute path is written out."
  }
};
