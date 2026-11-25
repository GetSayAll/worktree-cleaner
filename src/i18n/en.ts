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

// Kept as a module constant so the plan blocker banner and the exported plan
// use the same sentence.
const blockerPrefix = "Protected path: ";

export const en = {
  // The words that differ by the machine the app runs on. A string writes
  // `{{trash}}` and `applyLocale` fills it in (see `./index.ts`), so the interface
  // names the one system the reader is on. `other` is a browser preview on a third
  // system: the generic noun rather than a guess.
  platform: {
    trash: { macos: "Trash", windows: "Recycle Bin", other: "system trash" },
    pathExample: { macos: "/Users/you/repo", windows: "C:\\Users\\you\\repo", other: "/path/to/your-repo" },
  },
  app: {
    name: "Worktree Cleaner",
    // Intentionally blank until the tagline is decided. The key stays so that
    // landing the copy later is a one-line edit and nothing else has to move.
    tagline: "",
    readOnlyNote:
      "Read-only scan · every deletion is checked again.",
    versionNote: "v{{version}}",
    sampleData: "Sample data",
    previewNote: "Browser preview",
    switchProject: "Switch project",
    noProject: "No project selected",
    pickRepository: "Pick a repository to start scanning",
    desktopOnly:
      "Open the desktop app to do this."
  },
  preview: {
    // Said on the pages that offer an action, because a button that looks available
    // and does nothing is the failure this build was rewritten to remove.
    notice:
      "Browser preview · sample data; cleanup is disabled."
  },
  nav: {
    project: "Project",
    scan: "Scan",
    overview: "Overview",
    worktrees: "Worktree",
    artifacts: "Build artifacts",
    hygiene: "Disk hygiene",
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
  // The rules behind the three verdicts, stated once on the overview and in
  // shortened form where the reader acts on them. The full decision tables live
  // in docs/PRODUCT_PLAN.md section 5; this is the reader-facing summary.
  rules: {
    title: "Cleanup rules",
    shortLabel: "Rule:",
    hint: "Every row follows the same rules; only Safe items are selected by default",
    safeBody:
      "The worktree is clean and its work is in the default branch; or the artifact is rebuildable or empty, and free of protected content",
    reviewBody:
      "Work is unmerged, files are untracked, recent activity could still be lost, or a check did not finish",
    unsafeBody: "Main worktree, uncommitted changes, credentials, a lock, tracked files, or the whitelist",
    defaultSelected: "Selected by default",
    defaultUnselected: "Not selected",
    defaultDisabled: "Cannot be selected",
    footer:
      "Worktrees are deleted permanently; their branches remain. Build artifacts go to the {{trash}} and can be restored.",
    worktreesSummary:
      "Only Safe items are selected by default; Review items must be selected one by one; Unsafe items cannot be selected",
    artifactsSummary:
      "Only rebuildable or empty artifacts are deleted; everything else needs review.",
    welcomeSummary:
      "After a scan, every item is Safe to delete, Needs review, or Must not delete; only Safe items are selected by default",
    planSummary: "Only selected items that still pass the check run."
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
      // The append-only replacement: the old `unpushed` code was Unsafe and is
      // retained only so audit logs written before the change still render.
      unpushed_branch: "has commits not pushed to the upstream branch",
      detached: "HEAD is detached",
      no_upstream: "has no upstream branch and is not merged",
      unmerged: "not merged into the default branch",
      untracked: "has untracked files",
      credentials: "contains what looks like a credential file",
      // No longer produced by a scan: a stash belongs to the repository, not to the
      // worktree being judged. Kept so that audit logs written before it was withdrawn
      // still render.
      stash: "has stash entries",
      recent_activity: "committed within the activity window",
      agent_active: "a Git command may be running",
      cannot_verify: "cannot complete the check",
      whitelisted: "matches a whitelist rule",
      prunable: "the working directory no longer exists",
      merged: "merged into the default branch",
      absorbed: "the changes this branch made are already in the default branch",
      // No longer produced: absorption is the content answer, and a difference is
      // `unmerged`. Kept so audit logs written before the change still render.
      absorbed_content_changed:
        "the change is in the default branch, but the files it touched changed afterwards",
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
  // is a reason code that the reason table below also knows. A raw Git or OS
  // error stays in the record's arguments for the log; it is not shown in the
  // reader's sentence. `{field}` is looked up in `fields` and removed before
  // this runs.
  detail: {
    yes: "yes",
    no: "no",
    // Keyed as family/name, not as the dotted code, for the same reason
    // `reasons` is: the test helpers walk a table by splitting a path on dots,
    // so a key that contains one would be read as two segments.
    codes: {
      worktree: {
        not_in_scan: "No longer present; nothing was executed.",
        changed: "changed since the plan was made: {before} → {after}; not deleted",
        no_longer_safe: "no longer safe to delete: {reason}",
        removed: "Removed",
        remnants:
          "Removed, but the directory is still on disk.",
        remove_failed:
          "Could not remove this worktree. It may have uncommitted or untracked files, or be in use.",
        pruned: "Leftover records cleaned up",
        prune_failed:
          "Could not clean up this worktree's Git records. Try again later."
      },
      artifact: {
        not_in_scan: "No longer present; nothing was executed.",
        not_a_known_kind:
          "No longer an artifact directory; not deleted.",
        changed: "changed since the plan was made: {before} → {after}; not deleted",
        no_longer_safe: "no longer safe to delete: {reason}",
        vanished: "this directory is no longer on disk, so there was nothing to delete",
        unmeasurable: "Size could not be measured; not deleted.",
        trashed: "moved to the {{trash}}",
        trash_failed:
          "The directory could not be moved to the {{trash}}. It may be in use, or the account may lack permission."
      },
      prunable: {
        metadata_only:
          "The directory was already gone. Only leftover records were cleaned up; no files were touched."
      },
      precondition: {
        failed: "Path check failed; not deleted."
      },
      bundled: {
        with_worktree: "Removed with its worktree."
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
    legacy: "Older record"
  },
  // What the user did, keyed by ActivityAction. The deletions live in their own
  // section above; these are the other half of "what happened on this machine",
  // and they are the half a reader has no other way to reconstruct.
  activity: {
    codes: {
      scan: {
        started: "Started a scan",
        finished: "Scan finished: {worktrees} worktrees, {artifacts} artifact directories",
        failed: "The scan did not finish. The operation log has the technical detail.",
        cancelled: "Cancelled the scan after {elapsed}"
      },
      project: {
        switched: "Switched to this project from {from}"
      },
      plan: {
        generated:
          "Generated a plan: {worktrees} worktrees, {artifacts} artifact directories, {total_bytes} to release",
        exported: "Copied the plan: {item_count} items"
      },
      settings: {
        changed: "Changed a setting"
      },
      cleanup: {
        confirmed:
          "Confirmed the cleanup: {worktrees} worktrees, {artifacts} artifact directories, {forced} with untracked files"
      },
      trash: {
        opened: "Opened {{trash}}"
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
    defaultBranch: "default branch",
    unsafeTooltip: "Must not delete items cannot be selected",
    mainWorktree: (name: string) => `${name} (main worktree)`,
    rescan: "Rescan",
    copied: "Copied",
    action: {
      worktreeRemove: "Remove Worktree",
      trashMove: "Move to {{trash}}",
      prune: "Clean up leftovers"
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
    stages: {
      worktrees: {
        label: "Find worktrees",
      },
      status: {
        label: "Check changes",
      },
      artifacts: {
        label: "Find build artifacts",
      },
      verify: {
        label: "Summarize",
      }
    }
  },
  errors: {
    picker: "The folder picker could not open. Type or paste the project path instead.",
    historyRead: "The local operation log could not be read. Check the log file's permissions.",
    openTrash: "The {{trash}} could not be opened. Open it manually.",
    openLogDirectory: "The log directory could not be opened. Check that the path still exists.",
    clearLog: "The log could not be cleared. Check the file's permissions.",
    execute: "The cleanup did not finish. Try again; if it still fails, read the operation log.",
    logWrite: (path: string) =>
      `The cleanup ran, but the operation log at ${path} could not be written, so the audit trail is incomplete. Check disk space and the log directory's permissions.`,
    scan: {
      empty: "Choose a project folder first.",
      notFound: (path: string) =>
        `This folder could not be found: ${path}. Check the path and try again.`,
      notRepository: (path: string) =>
        `${path} is not a Git repository, and it is not inside one. Choose the project folder that contains .git, or run git init first.`,
      gitUnavailable:
        "Git information could not be read. Check that the folder is accessible. The operation log has the technical detail.",
      unknown:
        "The scan did not finish. Try again; the operation log has the technical detail."
    }
  },
  welcome: {
    lede: "Scan a repository, list every worktree and build artifact, and explain what can be deleted and what must stay.",
    pathPlaceholder: "Absolute path, like {{path}}",
    chooseFolder: "Choose folder",
    chooseFolderTitle: "Open the system folder picker",
    chooseFolderUnavailable: "Type the path instead.",
    startScan: "Start scan",
    dropHint: "Or drop a project folder here.",
    promises: {
      readOnlyTitle: "Read-only scan",
      readOnlyBody:
        "The scan changes nothing.",
      evidenceTitle: "Verifiable reasons",
      evidenceBody:
        "Every verdict says why.",
      trashTitle: "Two deletion paths",
      trashBody:
        "Build artifacts can be restored from the {{trash}}; worktrees are permanent."
    },
    recentTitle: "Recent projects",
    recentHint: "Click to scan",
    lastScan: (when: string) => `Last scan ${when}`,
    neverScanned: "not scanned yet",
    recentEmpty: "No projects yet; pick one to start"
  },
  overview: {
    title: "Overview",
    subtitle: (branch: string) => `default branch ${branch}`,
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
        `${bytes} total · ${unknown} ${unknown === 1 ? "size" : "sizes"} unknown · Safe items are selected by default`,
      reviewLabel: "Needs review",
      reviewSub: "not merged / changed recently",
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
      `${n} selected items have unknown size; the total is a minimum.`,
    selectHint: "Select items on the Worktree or Build artifacts page; Safe items are selected by default",
    adjustSelection: "Adjust selection",
    generatePlan: "Generate cleanup plan"
  },
  worktrees: {
    title: "Worktree",
    mainWorkspaceBadge: "Main workspace · Protected",
    mainWorkspaceHint: (_branch: string) =>
      "The main workspace is the repository itself and cannot be deleted.",
    // `git worktree add` takes any path, so a worktree of the repository the user
    // picked can sit anywhere. The page says so rather than leaving the reader to
    // notice that a path is not under the directory at the top of the window.
    outsideRoot: "outside the selected directory",
    outsideRootHint:
      "Outside the selected directory, but still scanned and removable.",
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
      stash: "Repository stash",
      untrackedFiles: "Untracked files",
      directorySize: "Directory size"
    },
    stashEntries: (n: number) => `${n} entries`,
    prunableBanner:
      "The working directory is gone; only leftover records are cleaned up, and no files are touched.",
    untrackedLabel: "Untracked files (deleted with this worktree)",
    empty: "No worktrees match the current filters",
    upstreamGone: "working directory is gone",
    upstreamNone: "no upstream branch",
    upstreamCounts: (ahead: number, behind: number) => `upstream ahead ${ahead} / behind ${behind}`,
    mergeMain: "main worktree",
    mergeUnknown: (branch: string) => `cannot be determined whether merged into ${branch}`,
    mergeYes: (branch: string) => `merged into ${branch}`,
    mergeNo: (branch: string) => `not merged into ${branch}`,
    /** The ancestry answer is "no", but every commit's patch is already there. */
    mergeAbsorbed: (branch: string) =>
      `its changes are already in ${branch}`,
    stateGone: "directory is gone",
    stateDirty: (n: number) => `dirty ${n}`,
    stateClean: "clean",
    stateUntracked: (n: number) => `untracked ${n}`,
    stateLocked: "locked",
    selectedSummary: (worktrees: number, artifacts: number) =>
      `Selected ${worktrees} worktrees · ${artifacts} artifacts · Estimated reclaimable`,
    continueToArtifacts: "Continue to artifacts",
    generatePlan: "Generate cleanup plan",
    // Shown on this page because this is where a cleanup starts: a repository that
    // violates the disk-hygiene policy will refill with worktrees and caches after
    // every pass, so the cause has to sit next to the symptom.
    hygieneTitle: "This repository will keep refilling",
    hygieneBody: (n: number) =>
      `${n} disk-hygiene ${n === 1 ? "rule is" : "rules are"} broken; this repository will refill after cleanup.`,
    hygieneAction: "Open disk hygiene"
  },
  artifacts: {
    title: "Build artifacts",
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
    gitUnverified: "unknown",
    gitUntracked: "untracked",
    // The rows name their own fields, so there is no column header to label.
    // This prefix is what stands in for it: "Git ignored" rather than "ignored".
    gitLabel: "Git",
    empty: "No artifact directories are larger than the current minimum",
    selectedSummary: (artifacts: number, worktrees: number) =>
      `Selected ${artifacts} artifacts · ${worktrees} worktrees · Estimated reclaimable`,
    backToWorktrees: "Back to Worktree",
    generatePlan: "Generate cleanup plan"
  },
  // The disk-hygiene policy, as a checklist. One entry per rule of
  // docs/DISK_HYGIENE.md, one per situation the audit can report, and the wiring
  // that makes each rule true. The numbers in `states` arrive as arguments: the
  // audit sends what it measured and the sentence is written here, so a threshold
  // is stated in the reader's language rather than frozen in the scanner.
  hygiene: {
    title: "Disk hygiene",
    lede:
      "Read-only audit; nothing is deleted.",
    metOf: (met: number, total: number) => `${met} of ${total} rules met`,
    stats: {
      violation: "Rules broken",
      advisory: "Worth fixing",
      unchecked: "Not checked",
      met: "Rules met"
    },
    violationBanner: (rules: number) =>
      `${rules === 1 ? "One rule is" : `${rules} rules are`} broken; each finding names its path and fix.`,
    level: {
      ok: "Met",
      advisory: "Worth fixing",
      violation: "Broken",
      unknown: "Not checked"
    },
    rules: {
      L1: "Worktrees live inside the repository",
      L2: "One clone per repository",
      L3: "No nesting",
      L4: "Naming: repo-branch-YYYYMMDD",
      L5: "Ignore entries are anchored and committed",
      L6: "git clean -x and -X are forbidden",
      L7: "Excluded from editor and platform indexes",
      T1: "At most 50 worktrees",
      T2: "A merge closes its worktree",
      T3: "Seven idle days make a worktree stale",
      T4: "Nothing survives 30 days",
      T5: "A session cleans up what it created",
      B1: "Output lives in declared locations only",
      B11: "git status lists no generated path",
      B2: "One cache root, mirrored by worktree name",
      B3: "Shared reads, isolated writes",
      B4: "Disposable by construction",
      B5: "Ten gigabytes per cache slot",
      Q1: "Free space above 15%, frozen below 10%",
      Q2: "The cache stays under a tenth of the volume",
      Q3: "Refused at creation, not reported afterwards",
      X1: "Exceptions carry a rule and an expiry"
    },
    states: {
      L1: {
        outside_repository:
          "Outside {expected}; a worktree beside the repository is not removed when the repository is",
        inside_but_undeclared:
          "Inside the repository but not under {expected}, so Git sees it as ordinary untracked files",
      },
      L2: {
        not_checkable: "Cannot be checked from this repository.",
      },
      L3: {
        nested_worktree: "This worktree sits inside {parent}",
      },
      L4: {
        not_a_slug: "{name} is not lowercase and hyphen-separated",
        too_long: "{name} is {length} characters; the limit is {limit}",
        no_date_suffix: "{name} does not end with its creation date as YYYYMMDD",
        invalid_date: "{name} ends with a date that does not exist",
        missing_repo_prefix:
          "{name} does not start with {prefix}-, so it cannot be traced back to this repository",
      },
      L5: {
        missing_entry: "Nothing ignores the declared paths; {entry} is not in .gitignore",
        no_worktrees_yet:
          "{entry} is not ignored yet; there is no worktree today; the entry is needed before the first one",
        unanchored_pattern:
          "The pattern {pattern} at {source}:{line} has no leading slash, so it matches at any depth — a build inside a worktree quietly creates its own copy",
        local_only:
          "{pattern} lives only in {source}, which is not committed: nobody else's clone ignores either path",
        elsewhere:
          "The paths are ignored by {source}, which is outside this repository's own .gitignore",
        nested_worktree_dir:
          "A .worktree/ directory exists inside a worktree; only the repository root should have one",
      },
      L6: {
        not_checkable:
          "Cannot be checked from this repository.",
      },
      L7: {
        not_checkable:
          "Cannot be checked from this repository.",
      },
      T1: {
        over_cap: "{count} worktrees are registered; the cap is {cap}",
        near_cap: "{count} of {cap} registered; remove one before creating the next",
      },
      T2: {
        merged_still_present:
          "{branch} is already merged into the default branch; removing the worktree is part of finishing the change, not a separate task",
      },
      T3: {
        stale: "No commit and no uncommitted change for {days} days",
        stale_with_changes:
          "No commit for {days} days, and {dirty} changed with {untracked} untracked; reported, not removed: the work has to leave first",
      },
      T4: {
        past_ceiling: "Created {days} days ago; nothing survives {ceiling} days",
        age_unknown:
          "{count} worktrees have no creation time.",
      },
      T5: {
        not_checkable: "Cannot be checked from this repository.",
      },
      B1: {
        outside_declared:
          "This {kind} directory is outside build-cache/, so it cannot be audited, quotaed, or deleted from one place",
      },
      B11: {
        untracked_in_status: "git status lists this {kind} directory as untracked",
        tracked_in_git: "Git tracks files inside this {kind} directory",
      },
      B2: {
        no_cache_root:
          "There is no build-cache/ at the repository root, so every build writes wherever its tool defaults to",
        orphan_slot: "build-cache/{slot} has no worktree named {slot}",
        missing_slots: "{count} worktrees have no cache slot yet: {slots}",
        nested_cache_root: "A second build-cache/ exists inside a worktree",
      },
      B3: {
        not_checkable:
          "Cannot be checked from this repository.",
      },
      B4: {
        not_checkable:
          "Cannot be checked from this repository.",
      },
      B5: {
        slot_over_cap: "This slot holds {size}; the ceiling for one worktree is {cap_bytes}",
        slot_unmeasurable: "The slot size could not be measured.",
      },
      Q1: {
        frozen:
          "{percent} free ({available} of {total}); below 10% nothing new is created — no worktree, cache entry, or release artifact — until free space is back above 12%",
        warning: "{percent} free ({available} of {total}); between 10% and 15% this is a warning",
        space_unknown: "The volume's free space could not be read",
      },
      Q2: {
        over_share: "build-cache holds {size}, {percent} of the volume; the guide is a tenth",
        share_unknown:
          "{count} cache slots could not be measured; the share is a minimum.",
      },
      Q3: {
        not_checkable:
          "Cannot be checked from this repository.",
      },
      X1: {
        entry_without_expiry:
          "Line {line} names a path without a rule and an expiry date, so as written it waives nothing",
        expiry_not_a_date:
          "Line {line} gives {expires} as the expiry; it has to read YYYY-MM-DD",
        expired: "The waiver on line {line} expired on {expires}",
        dangling: "Line {line} waives a path that no longer exists"
      }
    },
    fixes: {
      L1: "Move it in: git worktree move <path> <repo>/.worktree/<repo>-<branch>-<YYYYMMDD>",
      L2: "Keep the canonical clone and make the other a worktree of it",
      L3: "Remove the inner worktree; a worktree never owns another one",
      L4: "Rename it with git worktree move, so the directory carries the branch and the date",
      L5: "Add both entries to .gitignore, anchored, and commit them",
      L6: "Install the shell wrapper below; it is the only hard gate this rule can have",
      L7: "Add both paths to the editor's search exclusions and the platform index's ignore list",
      T1: "Remove a worktree before creating the next one",
      T2: "git worktree remove -- <path>, then delete build-cache/<name>/",
      T3: "Remove it — or push the branch first if it holds work",
      T4: "Push the branch or export a patch, then remove it; the ceiling is not negotiable",
      T5: "Remove it before the session ends, or hand it over by naming it and the next action",
      B1: "Point the build at its slot through WORKTREE_BUILD_CACHE, then delete this directory",
      B11: "Ignore it, or move it under build-cache/ and ignore it there",
      B2: "Create the slot the worktree writes to, or delete the slot nothing owns",
      B3: "Share resolved dependencies from build-cache/shared/ and keep compiled output per worktree",
      B4: "Make sure nothing reads the slot as an input; then it can be deleted at any time",
      B5: "Clear the slot; a cache may be deleted at any time without warning (B4)",
      Q1: "Free space before creating anything else",
      Q2: "Evict an idle worktree's slot before creating a new one",
      Q3: "Check T1, Q1, and Q2 before creating anything",
      X1: "Write the rule and the expiry date on the line, or delete the line"
    },
    findings: {
      title: "Findings",
      empty: "Every rule this scan can check is met",
      evidence: "Evidence",
      fix: "Fix",
      acceptedTitle: "Accepted exceptions",
      acceptedUntil: (date: string) =>
        `Accepted until ${date}`
    },
    setup: {
      title: "Set up before the first worktree",
      note:
        "The snippets below can be pasted as they are.",
      items: {
        ignore: "Both ignore entries, anchored and committed",
        cache: "One cache root, and the variable that points builds at it",
        guard: "A shell wrapper that refuses git clean -x and -X",
        close: "One command that closes a worktree together with its slot"
      }
    },
    snippets: {
      gitignore: "/.worktree/\n/build-cache/\n",
      cache:
        "export WORKTREE_BUILD_CACHE=\"$PWD/build-cache/<worktree-name>\"\nexport CARGO_TARGET_DIR=\"$WORKTREE_BUILD_CACHE/target\"\n",
      guard:
        "git() {\n  local arg wants_clean=0\n  for arg in \"$@\"; do\n    [ \"$arg\" = clean ] && wants_clean=1\n  done\n  if [ \"$wants_clean\" = 1 ]; then\n    for arg in \"$@\"; do\n      case \"$arg\" in\n        -*x*|*-*X*|--ignored*)\n          printf 'refused: git clean %s would delete .worktree/ (L6)\\n' \"$arg\" >&2\n          return 1 ;;\n      esac\n    done\n  fi\n  command git \"$@\"\n}\n",
      close:
        "git worktree remove -- \"<repo>/.worktree/<name>\"\n# then move build-cache/<name>/ to the {{trash}}\n"
    },
    copy: "Copy",
    scope: {
      title: "What this audit cannot see",
      note:
        "These rules depend on information outside this repository, so they are reported as not checked."
    },
  },
  plan: {
    title: "Cleanup plan",
    lede: "Review before running. Nothing has changed yet.",
    backToAdjustSelection: "Back to adjust selection",
    stats: {
      removeWorktree: "Remove Worktree",
      removeWorktreeSub: "Permanent",
      moveToTrash: "Move to {{trash}}",
      moveToTrashSub: (bundled: number) =>
        `plus ${bundled} ${bundled === 1 ? "item" : "items"} deleted with their worktree (not recoverable)`,
      reclaimable: "Estimated reclaimable",
      reclaimableSub: "Measured at scan time.",
      // The total omits items whose size could not be measured, so it is a lower
      // bound; the count keeps that visible instead of implying an exact figure.
      reclaimableSubUnknown: (n: number) =>
        `${n} ${n === 1 ? "item has" : "items have"} unknown size; the total is a minimum.`,
      autoExcluded: "Auto-excluded",
      autoExcludedSub: "Must-not-delete items never enter the plan."
    },
    blockerPrefix,
    blocker: (path: string) => `${blockerPrefix}${path}`,
    blockerTitle: "Blocked · ",
    ack: {
      force: (n: number) =>
        `Includes ${n} ${n === 1 ? "worktree" : "worktrees"} with untracked files; those files will be deleted too.`,
      // The count threshold is enforced in state.ts; the copy only states the
      // size of the run. The size limit arrives already formatted, so the unit
      // follows the active language.
      count: (n: number) =>
        `This run will process ${n} ${n === 1 ? "item" : "items"}.`,
      size: (limit: string) => `A single run frees more than ${limit}`,
      // With unmeasurable items in the plan the total is a lower bound, so the
      // size threshold above cannot be evaluated: this reason gates the run on
      // its own instead of guessing.
      unmeasurable: (n: number) =>
        `Includes ${n} ${n === 1 ? "item" : "items"} of unknown size; the total is a minimum.`
    },
    checks: {
      title: "Pre-run checks",
      note: "A dash means the check runs again before deletion.",
      recheckNotice:
        "Items are checked again before deletion; changed items are skipped.",
      pathCheck: (n: number) =>
        `Paths checked (${n} ${n === 1 ? "item" : "items"})`,
      whitelistPassed: "Protection check passed",
      whitelistBlocked: (n: number) =>
        `${n} ${n === 1 ? "item" : "items"} protected and excluded`,
      duplicateCheck: (n: number) =>
        `Artifacts removed with a worktree are counted once (${n} ${n === 1 ? "item" : "items"}).`,
      // The two deferred checks say what will be verified before deletion, so a
      // selected item is not mistaken for a guaranteed deletion.
      identityNotRun: "Re-check path and size before deletion.",
      activityNotRun: "Check for new changes before deletion."
    },
    channelA: "Remove worktrees permanently",
    items: (n: number) => `${n} ${n === 1 ? "item" : "items"}`,
    noWorktrees: "No worktrees selected",
    needForce: "Has untracked files",
    reviewItem: "Needs review",
    removable: "Removable directly",
    forceNote: (n: number) =>
      `These ${n} untracked files will be deleted too and cannot be recovered:`,
    prunableNote:
      "The working directory is gone; only leftover records are cleaned up, and no files are touched.",
    cleanNote: "The working tree is clean; no unsaved content will be lost",
    channelB: "Move artifact directories to the {{trash}}",
    trashHeader: (n: number, bytes: string) => `${n} ${n === 1 ? "item" : "items"} · ${bytes}`,
    columns: {
      directory: "Directory",
      files: "Files",
      size: "Size",
      recoverability: "Recoverability"
    },
    removedWithWorktree: "Deleted with worktree (not recoverable)",
    recoverable: "Recoverable from {{trash}}",
    noArtifacts: "No artifact directories selected",
    confirmationTitle: "Confirmation before execution",
    confirmationIntro:
      "This cleanup needs your confirmation:",
    permanenceNote:
      "Worktrees cannot be recovered from the {{trash}}; build artifacts can.",
    ackConfirm: (n: number) =>
      n > 0
        ? `I reviewed the list, including the untracked files that will be deleted in ${n} ${n === 1 ? "worktree" : "worktrees"}. I understand that worktree removal is permanent.`
        : "I reviewed the list. I understand that worktree removal is permanent.",
    confirmRequired: "Required confirmation",
    confirmed: "Checked",
    confirmHint: "Check this box to enable cleanup",
    confirmRules:
      "Worktrees are deleted permanently and their branches remain. Build artifacts go to the {{trash}} and can be restored.",
    typeNameBefore: "Type the project directory name",
    typeNameAfter: "to proceed",
    exportTitle: "Plan",
    copy: "Copy",
    hideExport: "Hide export",
    exportPlan: "Copy plan",
    excludedNote: (n: number) =>
      `${n} must-not-delete items were auto-excluded; return to the list to see why`,
    backToAdjust: "Back to adjust",
    runCleanup: "Run cleanup",
    running: "Running…",
    runningNote:
      "Keep the app open.",
  },
  result: {
    title: "Result",
    successTitle: "Cleanup complete",
    successSummary: (worktrees: number, artifacts: number, size: string) =>
      `Removed ${worktrees} worktrees, moved ${artifacts} artifact directories to the {{trash}}, and freed ${size}`,
    partialTitle: "Cleanup finished with skipped items",
    partialSummary: (worktrees: number, artifacts: number, size: string, failed: number) =>
      `Removed ${worktrees} worktrees, moved ${artifacts} artifact directories to the {{trash}}, and freed ${size}; ${failed} selected items were not deleted; the table below says why`,
    openTrash: "Open {{trash}}",
    trashNotice:
      "Build artifacts can be restored from the {{trash}}; worktrees cannot.",
    noTrashNotice:
      "Nothing went to the {{trash}}; the table below says why.",
    worktreePermanentNotice: (n: number) =>
      `${n} ${n === 1 ? "worktree was" : "worktrees were"} permanently deleted. ${n === 1 ? "Its branch remains; its working directory and untracked files do not." : "Their branches remain; their working directories and untracked files do not."}`,
    openLog: "Open log",
    logFailedTitle: "The operation log could not be written",
    stats: {
      spaceFreed: "Space freed",
      spaceFreedSub: "includes {{trash}} moves and worktrees",
      // Items that could never be measured contribute nothing to `freed`; saying
      // "at least" keeps the number honest instead of reporting a false total.
      spaceFreedSubUnknown: (n: number) =>
        `At least the measured sizes · ${n} items had no measurable size`,
      // A count that mixes worktrees and directories cannot be checked against the
      // Trash: worktree removal deletes in place and never goes through it.
      // Saying so here is what stops "4 succeeded, 1 in the Trash" looking wrong.
      succeeded: "Handled",
      succeededSub: (trashed: number) =>
        `${trashed} went to the {{trash}}; worktrees do not.`,
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
      "Failed items do not affect the others.",
    complete: "Cleanup complete; rescan to confirm the result",
    viewHistory: "View history",
    backToOverview: "Back to overview"
  },
  history: {
    title: "History",
    lede: "Logs are stored locally and never uploaded; see Settings for the location",
    copyLog: "Copy log",
    filterAll: (n: number) => `All ${n}`,
    filterWorktreeRemove: "Remove Worktree",
    filterTrashMove: "Move to {{trash}}",
    empty: "No records",
    reason: {
      confirmed: "Executed after user confirmation",
      rebuildable: "Rebuildable artifact",
      metadata: "Leftover records cleaned up"
    }
  },
  settings: {
    title: "Settings",
    scanScopeTitle: "Scan scope",
    extraScanPaths: "Extra scan paths",
    extraScanPathsHint:
      "One per line; use absolute paths, like {{path}}",
    maxDepth: "Maximum scan depth",
    maxDepthHint: "Deeper scans take longer.",
    perDirTimeout: "Time limit per directory",
    perDirTimeoutHint:
      "Timed-out items need review.",
    totalBudget: "Total scan time limit",
    totalBudgetHint: "Stops at the limit; unscanned items need review.",
    thresholdsTitle: "Criteria",
    activityWindow: "Quiet period (days)",
    activityWindowHint:
      "Items changed within this period need review.",
    artifactThreshold: "Minimum artifact size",
    artifactThresholdHint: "Smaller artifacts are hidden.",
    untrackedUnsafe: "Treat untracked files as unsafe",
    untrackedUnsafeHint:
      "When enabled, a worktree containing untracked files is marked Must not delete",
    protectionTitle: "Protection rules",
    whitelistPaths: "Whitelist paths",
    whitelistPathsHint: "Matching directories are never deleted.",
    protectedBranches: "Protected branches",
    protectedBranchesHint:
      "Comma-separated; the default branch is added automatically.",
    deletionTitle: "Deletion method",
    directoryDeletion: "Artifact directory deletion",
    directoryDeletionHint:
      "Artifact directories always go to the {{trash}}.",
    directoryDeletionOption: "{{trash}}",
    worktreeDeletion: "Worktree deletion",
    worktreeDeletionHint:
      "Deletes the working directory permanently and keeps the branch.",
    worktreeDeletionOption: "Permanent delete",
    strongProtection: "Strong protection",
    strongProtectionHint:
      "Type the project directory name before cleanup.",
    logsTitle: "Logs, language and appearance",
    operationLog: "Operation log",
    operationLogHint: (n: number) => `Written locally and never uploaded; ${n} records so far`,
    operationLogUnknown: "No log folder on this platform.",
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
    pathAbbrevAfter: "; the full absolute path is written out"
  },
  update: {
    title: "Updates",
    status: "Status",
    currentVersion: "Installed version",
    hint: "The app checks for a newer version when it starts; the request carries nothing about you or your files",
    check: "Check for updates",
    checking: "Checking for updates…",
    upToDate: "This is the latest version",
    available: (version: string) => `Version ${version} is available`,
    download: "Download and install",
    downloading: (percent: number) => `Downloading… ${percent}%`,
    downloadingUnknown: "Downloading…",
    ready: (version: string) => `Version ${version} is installed; restart to run it`,
    readyWindows:
      "The new version is installed; the installer closed the app; open it again to run it",
    restart: "Restart now",
    failed: (message: string) => `The check failed: ${message}`,
    installFailed: (message: string) => `The installation failed: ${message}`,
    banner: (version: string) => `Update ${version} available`,
    desktopOnly: "Only the desktop app checks for updates"
  }
};
