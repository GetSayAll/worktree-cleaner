// 简体中文消息表 —— Worktree Cleaner 的一个译文。
//
// 键名与 en.ts 完全一致，文件名与键名都用英文（开发契约 D23）：语言规则只
// 放行 src/i18n/ 下的 locale 数据，而「键集合必须与 en.ts 相同」由
// i18n/locale.test.ts 断言。新增或改动文案时，两个表必须同一次改完。
//
// 这里只放界面文案。扫描器产出的 reason text / evidence 是数据，不是界面
// 文案，仍随扫描结果留在 src/mock/snapshot.ts 与 Rust 侧。

// 与 en.ts 同名同义：计划页的拦截提示和导出的计划共用这一句。
const blockerPrefix = "受保护路径：";

export const zhCN = {
  // 随运行机器变化的词。字符串里写 `{{trash}}`，由 `applyLocale` 填入本机该用的
  // 那个词（见 `./index.ts`），所以界面上只出现读者所在的那一个系统，不并列两个。
  // `other` 是第三方系统上的浏览器预览：用通用说法，不猜。
  platform: {
    trash: { macos: "废纸篓", windows: "回收站", other: "系统回收站" },
    fileManager: { macos: "Finder", windows: "文件资源管理器", other: "文件管理器" },
    pathExample: { macos: "/Users/you/repo", windows: "C:\\Users\\you\\repo", other: "/path/to/your-repo" },
  },
  app: {
    name: "Worktree Cleaner",
    // 暂时留空，等口号定下来。键先留着：文案落地时只需改这一处，别的地方
    // （侧栏布局、两表一致性测试）都不用动。
    tagline: "",
    readOnlyNote:
      "只读扫描 · 每次删除前再次确认。",
    versionNote: "v{{version}}",
    sampleData: "示例数据",
    previewNote: "浏览器预览",
    switchProject: "切换项目",
    noProject: "尚未选择项目",
    pickRepository: "选择一个仓库开始扫描",
    desktopOnly: "请在桌面应用中操作。"
  },
  preview: {
    notice:
      "浏览器预览 · 示例数据；清理操作不可用。"
  },
  nav: {
    project: "项目",
    scan: "扫描",
    workspace: "仓库列表",
    caches: "系统缓存",
    buildCaches: "构建缓存",
    overview: "总览",
    worktrees: "工作树",
    artifacts: "构建产物",
    hygiene: "磁盘卫生",
    plan: "清理计划",
    result: "执行结果",
    history: "历史记录",
    settings: "设置"
  },
  status: {
    label: {
      Safe: "可安全删除",
      Review: "需人工确认",
      Unsafe: "禁止删除"
    },
    short: {
      Safe: "可删",
      Review: "待确认",
      Unsafe: "禁止"
    }
  },
  // 三种判定背后的规则：总览页完整写一次，其他页面只写读者当场要做的那一条。
  // 完整判定表在 docs/PRODUCT_PLAN.md 第 5 节，这里是面向读者的摘要。
  rules: {
    title: "清理规则",
    shortLabel: "规则：",
    hint: "每一行按同一套规则判定；默认只勾选「可安全删除」",
    safeBody: "工作区干净且工作已在默认分支；或可重建或为空、没有受保护内容的构建产物",
    reviewBody: "有未合并的工作、未跟踪文件、可能丢失的近期活动，或检查未完成",
    unsafeBody: "主工作树、未提交改动、凭据、锁定、已跟踪文件或白名单",
    defaultSelected: "默认勾选",
    defaultUnselected: "默认不选",
    defaultDisabled: "不可勾选",
    footer:
      "工作树永久删除，分支保留；构建产物进{{trash}}，可以恢复。",
    worktreesSummary:
      "默认只勾选「可安全删除」；「需人工确认」必须逐项勾选；「禁止删除」不可选",
    artifactsSummary:
      "只删除可重建或为空的产物；其他项留待确认。",
    welcomeSummary:
      "扫描后，每一项会判为「可安全删除」「需人工确认」或「禁止删除」；默认只勾选「可安全删除」",
    planSummary: "只执行已勾选且仍符合条件的条目。"
  },
  // 每个 ReasonCode 一条，顺序与 `src/types.ts` 一致。核心送来的判定理由因此
  // 用读者自己的语言显示，而不是送出它那个进程的语言。`evidence` 不在这里：
  // 那是命令、计数或路径，保持原样。
  //
  // `wt.cannot_verify` 在核心里有四种说法（「cannot read the worktree status」、
  // 「cannot read the HEAD commit time」、「cannot read the merge status」、
  // 「cannot read {label}」），但结论完全一样，所以一条就够，细节留给下面的证据行。
  reasons: {
    wt: {
      main: "这是主工作树",
      locked: "已被 Git 锁定",
      dirty: "有未提交的改动",
      unpushed: "有从未推送过的提交",
      unpushed_branch: "有未推送到上游分支的提交",
      detached: "HEAD 处于游离状态",
      no_upstream: "没有上游分支，且尚未合并",
      unmerged: "未合并进默认分支",
      untracked: "含有未跟踪文件",
      credentials: "含有疑似凭据文件",
      // 扫描不再产生：stash 属于仓库而不属于被判定 worktree。保留只为让旧审计日志仍能渲染。
      stash: "存在 stash 条目",
      recent_activity: "活动时间窗口内有过提交",
      agent_active: "可能有 Git 命令正在运行",
      cannot_verify: "无法完成检查",
      whitelisted: "命中白名单规则",
      prunable: "工作目录已不存在",
      merged: "已合并进默认分支",
      absorbed: "该分支的改动已在默认分支中",
      // 扫描不再产生：吸收判定改为内容判定，内容不同即 unmerged。保留只为让旧审计日志仍能渲染。
      absorbed_content_changed: "改动已在默认分支中，但相关文件之后又改过",
      clean: "工作区干净"
    },
    af: {
      git_tracked: "该目录含有已提交进仓库的文件",
      nested_git: "含有嵌套的 Git 仓库",
      authored_content: "含有手写内容",
      credentials: "含有疑似凭据文件",
      symlink_outside: "软链接指向项目之外",
      whitelisted: "受白名单规则保护",
      recent_activity: "最近有过改动",
      cannot_verify: "无法完成检查",
      empty: "该目录为空",
      git_ignored: "已被 .gitignore 忽略",
      rebuildable: "可重新生成的目录",
      unknown_cache: "缓存所属应用未知"
    }
  },
  // 一次执行动作的结果，按 DetailCode（`src/types.ts`）索引。核心决定「发生了什么」
  // 和原始值，句子写在这里 —— 所以这一列跟着界面语言走，而不是永远英文。
  //
  // `{before}` 与 `{after}` 是发生变化的字段的两侧，`{reason}` 是原因码（下面
  // 的 reasons 表认识它）。Git 或系统的原始错误只留在记录的参数和日志里，不进入
  // 读者看到的句子。`{field}` 在渲染前已从 `fields` 表查好并替换掉。
  detail: {
    yes: "是",
    no: "否",
    // 按 family/name 两级存，而不是直接用带点的码，理由与 reasons 表相同：测试
    // 用点号拆路径，键里带点会被读成两段。
    codes: {
      worktree: {
        not_in_scan: "已不存在，未执行任何操作。",
        changed: "生成计划后 {field} 变了：{before} → {after}；未删除",
        no_longer_safe: "不再判定为可安全删除：{reason}",
        removed: "已移除",
        remnants: "已移除，但目录仍在磁盘上。",
        remove_failed:
          "无法删除这个工作树。可能有未提交改动、未跟踪文件，或文件正在被使用。",
        pruned: "已清理残留记录",
        prune_failed: "无法清理这个工作树的 Git 记录。请稍后重试。"
      },
      artifact: {
        not_in_scan: "已不存在，未执行任何操作。",
        not_a_known_kind: "已不是构建产物目录，未删除。",
        changed: "生成计划后 {field} 变了：{before} → {after}；未删除",
        no_longer_safe: "不再判定为可安全删除：{reason}",
        vanished: "该目录已不在磁盘上，无需删除",
        unmeasurable: "无法测量体积，未删除。",
        trashed: "已移入{{trash}}",
        trash_failed: "无法把这个目录移入{{trash}}。可能被其他程序占用，或当前账户没有权限。"
      },
      prunable: {
        metadata_only: "该目录早已不存在，只清理了残留记录，未触碰任何文件。"
      },
      precondition: {
        failed: "路径检查未通过，未执行。"
      },
      bundled: {
        with_worktree: "已随所属工作树一起移除。"
      }
    },
    // 变化字段的名字，供两个 changed 码使用。没有这张表，句子里的字段名会是英文。
    fields: {
      level: "判定结果",
      prunable: "工作目录",
      dirty: "未提交改动",
      untracked: "未跟踪文件",
      merged: "合并状态",
      size: "体积",
      files: "文件数",
      tracked: "Git 跟踪状态",
      ignored: "忽略规则",
      symlink: "软链接"
    },
    legacy: "旧版本记录"
  },
  // 用户做过什么，按 ActivityAction 索引。删除记录在上面自己的段里；这里是
  // 「这台机器上发生过什么」的另一半，也是读者没有别的办法重建的那一半。
  activity: {
    codes: {
      scan: {
        started: "开始扫描",
        finished: "扫描完成：{worktrees} 个工作树、{artifacts} 个产物目录",
        failed: "扫描没有完成。技术详情见操作日志。",
        cancelled: "扫描在 {elapsed} 后被取消"
      },
      workspace: {
        discovered: "在这个文件夹中找到了 {repositories} 个仓库"
      },
      project: {
        switched: "从 {from} 切换到这个项目"
      },
      plan: {
        generated: "生成了清理计划：{worktrees} 个工作树、{artifacts} 个产物目录，预计释放 {total_bytes}",
        exported: "已复制计划：{item_count} 项"
      },
      settings: {
        changed: "修改了一项设置"
      },
      cleanup: {
        confirmed: "确认执行清理：{worktrees} 个工作树、{artifacts} 个产物目录，其中 {forced} 个含未跟踪文件"
      },
      trash: {
        opened: "打开{{trash}}"
      }
    },
    kinds: {
      deletion: "删除了",
      action: "你的操作"
    },
    filterAction: "你的操作"
  },
  common: {
    unknown: "未知",
    none: "（无）",
    emDash: "—",
    detached: "（游离）",
    detachedHead: "（游离 HEAD）",
    defaultBranch: "默认分支",
    unsafeTooltip: "禁止删除的条目无法勾选",
    unknownAgent: "未知代理",
    unknownAgentHint:
      "路径和分支里都没有代理标记。这个标签只用于说明，不影响删除安全判断。",
    reveal: "在{{fileManager}}中显示",
    revealShort: "↗",
    revealHint: "在{{fileManager}}中显示这个项目",
    mainWorktree: (name: string) => `${name}（主工作树）`,
    rescan: "重新扫描",
    copied: "已复制",
    action: {
      worktreeRemove: "移除工作树",
      trashMove: "移入{{trash}}",
      prune: "清理残留"
    },
    outcome: {
      ok: "已删除",
      // "Skipped" said nothing about whether anything happened. Every row on this
      // page is a deletion, so the reader's first question is whether the thing is
      // gone — "not deleted" answers it, and the row's own detail says why.
      skipped: "未删除",
      failed: "删除失败"
    }
  },
  units: {
    bytes: "B",
    kilobytes: "KB",
    megabytes: "MB",
    gigabytes: "GB",
    terabytes: "TB",
    atLeast: "≥ ",
    seconds: (n: number) => `${n} 秒`,
    fileCount: (count: string) => `${count} 个文件`
  },
  time: {
    justNow: "刚刚",
    hoursAgo: (n: number) => `${n} 小时前`,
    oneDayAgo: "1 天前",
    daysAgo: (n: number) => `${n} 天前`,
    monthsAgo: (n: number) => `${n} 个月前`,
    yearsAgo: (n: number) => `${n} 年前`
  },
  scan: {
    title: "正在扫描",
    elapsed: (clock: string) => `已用时 ${clock}`,
    cancel: "取消",
    scanning: "扫描中…",
    stages: {
      worktrees: {
        label: "查找工作树",
      },
      status: {
        label: "检查改动",
      },
      artifacts: {
        label: "查找构建产物",
      },
      verify: {
        label: "汇总",
      }
    }
  },
  errors: {
    noRepositories: (path: string) =>
      `${path} 下面没有找到 Git 仓库。请选择一个仓库，或选择包含仓库的文件夹。`,
    picker: "无法打开文件夹选择器。请手动输入或粘贴项目路径。",
    historyRead: "无法读取本地操作日志。请检查日志文件的权限。",
    openTrash: "无法打开{{trash}}。请手动打开。",
    openLogDirectory: "无法打开日志目录。请检查路径是否仍然存在。",
    clearLog: "无法清空日志。请检查文件权限。",
    reveal: "无法显示这个项目。它可能已被移动或删除。",
    execute: "清理没有执行完成。请重试；如果仍然失败，请查看操作日志。",
    logWrite: (path: string) =>
      `清理已经执行，但操作日志 ${path} 写入失败，审计记录不完整。请检查磁盘空间和日志目录权限。`,
    scan: {
      empty: "请先选择一个项目文件夹。",
      notFound: (path: string) => `找不到这个文件夹：${path}。请检查路径后重试。`,
      notRepository: (path: string) =>
        `${path} 不是 Git 仓库，也不在某个 Git 仓库里面。请选择包含 .git 的项目目录，或先运行 git init。`,
      gitUnavailable:
        "无法读取这个文件夹的 Git 信息。请确认文件夹可访问。技术详情见操作日志。",
      unknown: "扫描没有完成。请重试；技术详情见操作日志。"
    }
  },
  workspace: {
    title: "仓库列表",
    lede:
      "这个文件夹里包含多个 Git 仓库。每个仓库单独扫描、单独清理，所有工作树判定仍然以它自己的默认分支为准。",
    chooseAnother: "选择其他文件夹",
    repositoriesTitle: (count: number) => `仓库（${count}）`,
    oneAtATime: "一次处理一个仓库",
    worktrees: (count: number | null) =>
      count === null ? "工作树数量未知" : count === 1 ? "1 个工作树" : `${count} 个工作树`,
    scan: "扫描这个仓库",
    rowEvidence: "只读发现；扫描这个仓库后才会评估它的工作树",
    empty: "这个文件夹下面没有找到 Git 仓库。"
  },
  caches: {
    title: "系统缓存",
    lede:
      "这是一个独立的只读缓存清单，只列出已知缓存根目录。只有你明确选中的目录才会移入{{trash}}；不会改动源码目录。",
    rulesSummary:
      "已知缓存目录为可安全删除；无法识别所属应用的缓存为需确认，且默认不勾选。",
    scanning: "正在扫描缓存…",
    rescan: "重新扫描缓存",
    scanFailed: "缓存清单没有扫描完成。请重试；技术详情见操作日志。",
    stats: {
      directories: "缓存目录",
      directoriesSub: "只读清单",
      bytes: "缓存体积",
      bytesSub: "所有已列出缓存的总和",
      bytesUnknown: (count: number) => `${count} 个目录无法测量体积`,
      selected: "已选择",
      selectedSub: (bytes: string) => `将移入{{trash}}：${bytes}`
    },
    inventoryTitle: "已知缓存根目录",
    inventoryHint: "只显示超过产物体积阈值的缓存；无法识别所属应用的缓存需要你明确确认",
    selectAllKnown: "选中全部已知安全缓存",
    unknownOwner: "所属应用未知",
    files: "个文件",
    empty: "没有找到达到体积阈值的缓存目录。",
    planTitle: "缓存清理计划",
    planHint: "每个选中路径都会在移动前重新检查",
    planLede: "这些目录会移入{{trash}}；先关闭所属应用，避免移动它仍在写入的文件。",
    noSelection: "选择一个缓存目录来生成计划。",
    planTotal: (bytes: string) => `已选体积：${bytes}`,
    ackConfirm: "我已核对选中的缓存目录；它们会移入{{trash}}，之后可以恢复。",
    execute: "把选中项移入{{trash}}",
    running: "正在移动…",
    resultTitle: "缓存清理结果",
    openTrash: "打开{{trash}}",
    resultSummary: (moved: number, bytes: string, failed: number) =>
      `已把 ${moved} 个缓存目录移入{{trash}}（${bytes}）；有 ${failed} 个选中项未移动。`,
    columns: {
      directory: "目录",
      detail: "详情",
      result: "结果",
      size: "体积"
    },
    history: "查看历史记录",
    footer: "无法识别所属应用的缓存即使体积很大，也保持为需确认。",
    backToProject: "返回项目"
  },
  buildCaches: {
    title: "构建缓存",
    lede:
      "应用会从你上次扫描的文件夹开始，自动查找带有 CACHEDIR.TAG 标记的构建根目录，例如 SwiftPM 为每次构建生成的目录。你不需要知道哪个目录是缓存；只有你明确选中的构建根目录才会移入{{trash}}。",
    rulesSummary:
      "带标记的构建根目录是可重建缓存。凭据、最近活动、符号链接和白名单仍会使它需要确认或被禁止删除。",
    scopeTitle: "上次扫描的文件夹",
    scopeHint: "应用会自动在这个文件夹下面查找；只有缓存位于别处时才需要更换。",
    noScope: "还没有扫描过文件夹。先选择一个文件夹开始。",
    chooseFolder: "选择文件夹",
    scan: "扫描构建缓存",
    scanning: "正在扫描构建缓存…",
    rescan: "重新扫描构建缓存",
    scanFailed: "构建缓存清单没有扫描完成。请重试；技术详情见操作日志。",
    emptyRoot: "请先选择一个文件夹。",
    stats: {
      caches: "构建缓存",
      cachesSub: "带有 CACHEDIR.TAG 的目录",
      bytes: "测得体积",
      bytesSub: "所有已列出构建根目录的总和",
      bytesUnknown: (count: number) => `${count} 个构建根目录无法测量体积`,
      selected: "已选择",
      selectedSub: (bytes: string) => `将移入{{trash}}：${bytes}`
    },
    inventoryTitle: "带标记的构建根目录",
    inventoryHint: "只显示带有 CACHEDIR.TAG 的目录；仍受产物体积阈值限制",
    selectAllSafe: "选中全部可安全清理的构建缓存",
    files: "个文件",
    empty: "这个文件夹下面没有找到带 CACHEDIR.TAG 的目录。",
    planTitle: "构建缓存清理计划",
    planHint: "每个选中路径都会在移动前重新检查",
    planLede: "这些构建根目录会移入{{trash}}，之后可以恢复；请先关闭仍在使用它们的构建。",
    noSelection: "选择一个构建根目录来生成计划。",
    planTotal: (bytes: string) => `已选体积：${bytes}`,
    ackConfirm: "我已核对选中的构建根目录；它们会移入{{trash}}，之后可以恢复。",
    execute: "把选中项移入{{trash}}",
    running: "正在移动…",
    resultTitle: "构建缓存清理结果",
    openTrash: "打开{{trash}}",
    resultSummary: (moved: number, bytes: string, failed: number) =>
      `已把 ${moved} 个构建根目录移入{{trash}}（${bytes}）；有 ${failed} 个选中项未移动。`,
    columns: {
      directory: "目录",
      detail: "详情",
      result: "结果",
      size: "体积"
    },
    history: "查看历史记录",
    footer: "只有带标记的构建根目录在范围内；未标记目录需要单独的理由才能清理。",
    backToProject: "返回项目"
  },
  welcome: {
    lede: "扫描仓库，列出全部工作树和构建产物，并说明哪些能删、哪些不能删。",
    pathPlaceholder: "绝对路径，例如 {{path}}",
    chooseFolder: "选择文件夹",
    chooseFolderTitle: "打开系统文件夹选择器",
    chooseFolderUnavailable: "请直接输入路径。",
    startScan: "开始扫描",
    dropHint: "也可以把项目文件夹拖进这个窗口。",
    promises: {
      readOnlyTitle: "只读扫描",
      readOnlyBody:
        "扫描不会改动任何文件。",
      evidenceTitle: "可核验的理由",
      evidenceBody:
        "每条判定都写明理由。",
      trashTitle: "两条删除路径",
      trashBody:
        "构建产物可从{{trash}}恢复；工作树永久删除。"
    },
    recentTitle: "最近的项目",
    recentHint: "点击即可扫描",
    lastScan: (when: string) => `上次扫描 ${when}`,
    neverScanned: "尚未扫描",
    recentEmpty: "还没有项目记录，先选一个开始吧"
  },
  overview: {
    title: "总览",
    subtitle: (branch: string) => `默认分支 ${branch}`,
    viewWorktrees: "查看工作树",
    viewArtifacts: "查看构建产物",
    stats: {
      safeLabel: "可安全删除",
      safeSub: (bytes: string) => `共 ${bytes} · 可删条目默认勾选`,
      // 部分可删条目测量超时：字节数缺失，因此合计只是下界；这里如实报出无法
      // 测量的条数，而不是悄悄丢掉。`bytes` 参数已自带「≥」前缀
      // （formatBytes 的 approximate），所以这句不能重复写。
      safeSubUnknown: (bytes: string, unknown: number) =>
        `合计 ${bytes} · ${unknown} 项体积未知 · 可删条目默认勾选`,
      reviewLabel: "需人工确认",
      reviewSub: "未合并 / 最近有改动",
      unsafeLabel: "禁止删除",
      unsafeSub: "主工作树 / 有未提交改动 / 已锁定 / 已提交进仓库",
      reclaimableLabel: "预计可回收",
      reclaimableSub: (count: number, worktrees: number, artifacts: number) =>
        `已选 ${count} 项 · 工作树 ${worktrees} · 产物 ${artifacts}`
    },
    warningTitle: "扫描未完成 · ",
    largestWorktrees: "体积最大的工作树",
    viewAll: (n: number) => `查看全部 ${n}`,
    mainWorktreeSuffix: " · 主工作树",
    largestArtifacts: "体积最大的构建产物",
    artifactMeta: (ecosystem: string, files: string) => `${ecosystem} · ${files} 个文件`,
    selected: (n: number) => `已选 ${n} 项 · 预计可回收`,
    // 可回收卡片下方：选中项里有无法测量的，合计只是下界，用户必须一眼看到。
    selectedUnknown: (n: number) =>
      `${n} 项体积未知，合计为下限。`,
    selectHint: "在「工作树」或「构建产物」页面勾选；可删条目默认已勾选",
    adjustSelection: "调整选择",
    generatePlan: "生成清理计划"
  },
  worktrees: {
    title: "工作树",
    mainWorkspaceBadge: "主工作区 · 受保护",
    mainWorkspaceHint: (_branch: string) =>
      "主工作区就是仓库本身，不能删除。",
    outsideRoot: "在所选目录之外",
    outsideRootHint:
      "在所选目录之外，但仍可扫描和删除。",
    filterAll: (n: number) => `全部 ${n}`,
    filterSafe: (n: number) => `可安全删除 ${n}`,
    filterReview: (n: number) => `需人工确认 ${n}`,
    filterUnsafe: (n: number) => `禁止删除 ${n}`,
    searchPlaceholder: "按路径或分支搜索",
    selectAll: "全选可移除的工作树",
    details: "详情",
    collapse: "收起",
    gitFacts: "Git 事实",
    facts: {
      fullPath: "完整路径",
      head: "HEAD",
      upstream: "上游分支",
      mergeStatus: "合并状态",
      stash: "仓库级 stash",
      untrackedFiles: "未跟踪文件",
      directorySize: "目录体积"
    },
    stashEntries: (n: number) => `${n} 条`,
    prunableBanner:
      "工作目录已不在；只清理残留记录，不碰文件。",
    untrackedLabel: "未跟踪文件（删除时会一并删除）",
    empty: "没有符合当前筛选条件的工作树",
    upstreamGone: "工作目录已消失",
    upstreamNone: "没有上游分支",
    upstreamCounts: (ahead: number, behind: number) => `上游领先 ${ahead} / 落后 ${behind}`,
    mergeMain: "主工作树",
    mergeUnknown: (branch: string) => `无法判定是否已合并进 ${branch}`,
    mergeYes: (branch: string) => `已合并进 ${branch}`,
    mergeNo: (branch: string) => `未合并进 ${branch}`,
    mergeAbsorbed: (branch: string) =>
      `改动已在 ${branch} 中`,
    stateGone: "目录已消失",
    stateDirty: (n: number) => `有改动 ${n}`,
    stateClean: "干净",
    stateUntracked: (n: number) => `未跟踪 ${n}`,
    stateLocked: "已锁定",
    selectedSummary: (worktrees: number, artifacts: number) =>
      `已选 ${worktrees} 个工作树 · ${artifacts} 个产物 · 预计可回收`,
    continueToArtifacts: "继续查看构建产物",
    generatePlan: "生成清理计划",
    hygieneTitle: "这个仓库会不断重新堆积",
    hygieneBody: (n: number) =>
      `本仓库有 ${n} 条磁盘卫生规则未通过，清理后会继续堆积。`,
    hygieneAction: "查看磁盘卫生"
  },
  artifacts: {
    title: "构建产物",
    bySize: "按体积",
    byLastActivity: "按最近活动",
    showOnlyLargerThan: "只显示大于",
    summary: (total: number, safe: number, review: number, blocked: number) =>
      `共 ${total} · 可删 ${safe} · 待确认 ${review} · 禁止 ${blocked}`,
    groupMeta: (items: number, bytes: string) => `${items} 项 · ${bytes}`,
    selectAllSafe: "全选可删项",
    bundledTitle: "随所属工作树一起移除",
    bundledBadge: "随工作树移除",
    unowned: "（不属于任何工作树）",
    worktreeSuffix: (name: string) => `${name}（工作树）`,
    fileCountUnknown: "文件数未知",
    symlink: (target: string) => ` · 软链接 → ${target}`,
    gitTracked: "已跟踪",
    gitIgnored: "已忽略",
    gitUnverified: "未知",
    gitUntracked: "未跟踪",
    // 每一行自带字段名，没有表头可标注。这个前缀就是表头的替身：写「Git 已忽略」
    // 而不是孤零零一个「已忽略」。
    gitLabel: "Git",
    empty: "没有大于当前下限的产物目录",
    selectedSummary: (artifacts: number, worktrees: number) =>
      `已选 ${artifacts} 个产物 · ${worktrees} 个工作树 · 预计可回收`,
    backToWorktrees: "返回工作树",
    generatePlan: "生成清理计划"
  },
  // 磁盘卫生规范，做成一张清单。每条规则对应 docs/DISK_HYGIENE.md 里的一条；
  // 每种情形（states）对应审计能报出的一种情况；fixes 是可以照着做的那一步。
  // states 里的数字由审计以参数形式给出——审计只报告它量到了什么，句子写在这里，
  // 这样阈值是用读者选择的语言说出来的，而不是冻在扫描器里。
  hygiene: {
    title: "磁盘卫生",
    lede:
      "只读检查，不会删除任何东西。",
    metOf: (met: number, total: number) => `${total} 条规则中符合 ${met} 条`,
    stats: {
      violation: "违反的规则",
      advisory: "建议处理",
      unchecked: "未检查",
      met: "符合的规则"
    },
    violationBanner: (rules: number) =>
      `有 ${rules} 条规则未通过；下面每条都写明路径和处理办法。`,
    level: {
      ok: "符合",
      advisory: "建议处理",
      violation: "违反",
      unknown: "未检查"
    },
    rules: {
      L1: "工作树放在仓库内部",
      L2: "一个仓库只有一个 clone",
      L3: "禁止嵌套",
      L4: "命名：仓库-分支-YYYYMMDD",
      L5: "忽略条目必须锚定并提交",
      L6: "禁止 git clean -x 与 -X",
      L7: "从编辑器与系统索引中排除",
      T1: "工作树最多 50 个",
      T2: "合并即关闭工作树",
      T3: "闲置七天即为陈旧",
      T4: "任何工作树不超过 30 天",
      T5: "会话清理自己创建的东西",
      B1: "产物只能出现在声明位置",
      B11: "git status 不出现任何生成物",
      B2: "唯一的缓存根，按工作树名镜像",
      B3: "共享读，隔离写",
      B4: "天生可丢弃",
      B5: "单个缓存槽位 10 GB",
      Q1: "可用空间高于 15%，低于 10% 冻结",
      Q2: "缓存不超过卷容量的一成",
      Q3: "在创建时拒绝，而非事后报告",
      X1: "例外必须写明规则与失效日期"
    },
    states: {
      L1: {
        outside_repository:
          "位于 {expected} 之外；仓库被删除时，放在仓库旁边的工作树不会跟着消失",
        inside_but_undeclared:
          "在仓库内部，但不在 {expected} 之下，于是 Git 把它当作普通的未跟踪文件",
      },
      L2: {
        not_checkable: "无法从当前仓库检查。",
      },
      L3: {
        nested_worktree: "这个工作树位于 {parent} 内部",
      },
      L4: {
        not_a_slug: "{name} 不是小写、连字符分隔的形式",
        too_long: "{name} 有 {length} 个字符；上限是 {limit}",
        no_date_suffix: "{name} 没有以创建日期 YYYYMMDD 结尾",
        invalid_date: "{name} 结尾的日期并不存在",
        missing_repo_prefix: "{name} 没有以 {prefix}- 开头，无法追溯到本仓库",
      },
      L5: {
        missing_entry: "两个声明路径都没有被忽略；.gitignore 里没有 {entry}",
        no_worktrees_yet:
          "{entry} 还没有被忽略；目前没有工作树；这条要在创建第一个之前就写好",
        unanchored_pattern:
          "{source}:{line} 的模式 {pattern} 没有前导斜杠，会在任意层级匹配——某个工作树内部跑一次构建，就会悄悄长出属于它自己的一份",
        local_only:
          "{pattern} 只写在 {source} 里，而那个文件不会提交：别人的 clone 两个路径都没被忽略",
        elsewhere: "这些路径由 {source} 忽略，而它不在本仓库自己的 .gitignore 里",
        nested_worktree_dir: "某个工作树内部出现了 .worktree/ 目录；它只应存在于仓库根目录",
      },
      L6: {
        not_checkable: "无法从当前仓库检查。",
      },
      L7: {
        not_checkable: "无法从当前仓库检查。",
      },
      T1: {
        over_cap: "已注册 {count} 个工作树；上限是 {cap} 个",
        near_cap: "已注册 {count}/{cap} 个；创建下一个之前先移除一个",
      },
      T2: {
        merged_still_present:
          "{branch} 已经合入主线；移除该工作树是「完成这次改动」的一部分，不是另一件单独的任务",
      },
      T3: {
        stale: "连续 {days} 天既无新提交，也无未提交改动",
        stale_with_changes:
          "{days} 天没有提交，且仍有 {dirty} 处改动、{untracked} 个未跟踪文件；只报告，不删除：得先把成果带出去",
      },
      T4: {
        past_ceiling: "创建于 {days} 天前；任何工作树都不超过 {ceiling} 天",
        age_unknown: "{count} 个工作树无法读取创建时间。",
      },
      T5: {
        not_checkable: "无法从当前仓库检查。",
      },
      B1: {
        outside_declared:
          "这个 {kind} 目录在 build-cache/ 之外，因此无法在一个地方审计、限额和删除",
      },
      B11: {
        untracked_in_status: "git status 把这个 {kind} 目录列为未跟踪",
        tracked_in_git: "Git 跟踪着这个 {kind} 目录里的文件",
      },
      B2: {
        no_cache_root:
          "仓库根目录没有 build-cache/，于是每次构建都写到工具自己的默认位置去了",
        orphan_slot: "build-cache/{slot} 找不到名为 {slot} 的工作树",
        missing_slots: "有 {count} 个工作树还没有缓存槽位：{slots}",
        nested_cache_root: "某个工作树内部出现了第二个 build-cache/",
      },
      B3: {
        not_checkable: "无法从当前仓库检查。",
      },
      B4: {
        not_checkable: "无法从当前仓库检查。",
      },
      B5: {
        slot_over_cap: "这个槽位已占 {size}；单个工作树的上限是 {cap_bytes}",
        slot_unmeasurable: "无法测量这个槽位的体积。",
      },
      Q1: {
        frozen:
          "可用空间 {percent}（{available} / {total}）；低于 10% 时不得新建任何东西——工作树、缓存条目、发布产物都不行——直到可用空间回到 12% 以上",
        warning: "可用空间 {percent}（{available} / {total}）；处于 10%–15% 之间，属于告警",
        space_unknown: "读不到这个卷的可用空间",
      },
      Q2: {
        over_share: "build-cache 已占 {size}，为卷容量的 {percent}；建议控制在一成以内",
        share_unknown: "{count} 个缓存槽位无法测量，占比为下限。",
      },
      Q3: {
        not_checkable: "无法从当前仓库检查。",
      },
      X1: {
        entry_without_expiry:
          "第 {line} 行只写了路径，没有规则也没有失效日期；按现在这样写，它什么都豁免不了",
        expiry_not_a_date: "第 {line} 行把失效日期写成 {expires}；必须是 YYYY-MM-DD",
        expired: "第 {line} 行的豁免已于 {expires} 失效",
        dangling: "第 {line} 行豁免的路径已经不存在了"
      }
    },
    fixes: {
      L1: "迁进去：git worktree move <路径> <仓库>/.worktree/<仓库>-<分支>-<YYYYMMDD>",
      L2: "保留规范 clone，把另一个改成它的工作树",
      L3: "移除内层工作树；工作树不会拥有另一个工作树",
      L4: "用 git worktree move 改名，让目录名带上分支与日期",
      L5: "把两条条目加进 .gitignore，带前导斜杠，并提交",
      L6: "装上下面的 shell 包装器；这条规则唯一能有的硬拦截就是它",
      L7: "把两个路径加进编辑器的搜索排除与系统索引的忽略列表",
      T1: "先移除一个工作树，再创建下一个",
      T2: "git worktree remove -- <路径>，然后删除 build-cache/<名称>/",
      T3: "移除它——如果上面还有成果，先把分支推送出去",
      T4: "推送分支或导出 patch，然后移除；上限没有例外",
      T5: "在会话结束前移除，或明确交接：点名该工作树并说明下一步动作",
      B1: "用 WORKTREE_BUILD_CACHE 把构建指向它自己的槽位，然后删掉这个目录",
      B11: "把它忽略掉，或移到 build-cache/ 下再忽略",
      B2: "创建该工作树写入的槽位，或删掉没有归属的那个槽位",
      B3: "已解析的依赖放在 build-cache/shared/ 共享，编译产物按工作树隔离",
      B4: "确认没有任何东西把它当作输入读取；此后它随时可以删除",
      B5: "清空这个槽位；缓存任何时候都可以无预警删除（B4）",
      Q1: "先腾出空间，再创建别的东西",
      Q2: "先回收某个闲置工作树的槽位，再新建",
      Q3: "创建任何东西之前，先过 T1、Q1、Q2 这三关",
      X1: "在这一行补上规则与失效日期，或者把这一行删掉"
    },
    findings: {
      title: "问题",
      empty: "本次扫描能检查的规则全部符合",
      evidence: "依据",
      fix: "处理",
      acceptedTitle: "已接受的例外",
      acceptedUntil: (date: string) =>
        `已接受至 ${date}`
    },
    setup: {
      title: "在创建第一个工作树之前做好",
      note:
        "下面的片段可以直接粘贴。",
      items: {
        ignore: "两条忽略条目，带前导斜杠并提交",
        cache: "唯一的缓存根，以及把构建指向它的变量",
        guard: "拒绝 git clean -x / -X 的 shell 包装器",
        close: "一条把工作树连同槽位一起关闭的命令"
      }
    },
    snippets: {
      gitignore: "/.worktree/\n/build-cache/\n",
      cache:
        "export WORKTREE_BUILD_CACHE=\"$PWD/build-cache/<工作树名>\"\nexport CARGO_TARGET_DIR=\"$WORKTREE_BUILD_CACHE/target\"\n",
      guard:
        "git() {\n  local arg wants_clean=0\n  for arg in \"$@\"; do\n    [ \"$arg\" = clean ] && wants_clean=1\n  done\n  if [ \"$wants_clean\" = 1 ]; then\n    for arg in \"$@\"; do\n      case \"$arg\" in\n        -*x*|*-*X*|--ignored*)\n          printf 'refused: git clean %s would delete .worktree/ (L6)\\n' \"$arg\" >&2\n          return 1 ;;\n      esac\n    done\n  fi\n  command git \"$@\"\n}\n",
      close:
        "git worktree remove -- \"<仓库>/.worktree/<名称>\"\n# 然后把 build-cache/<名称>/ 移到{{trash}}\n"
    },
    copy: "复制",
    scope: {
      title: "这次审计看不到什么",
      note:
        "这些规则依赖仓库之外的信息，因此只报「未检查」。"
    },
  },
  plan: {
    title: "清理计划",
    lede: "执行前复核。尚未改动任何内容。",
    backToAdjustSelection: "返回调整选择",
    stats: {
      removeWorktree: "移除工作树",
      removeWorktreeSub: "永久删除",
      moveToTrash: "移入{{trash}}",
      moveToTrashSub: (bundled: number) => `另有 ${bundled} 项随所属工作树一并删除（不可恢复）`,
      reclaimable: "预计可回收",
      reclaimableSub: "扫描时测得的体积。",
      // 合计里缺少无法测量的条目，所以只是下界；这里把条数摆出来，避免把下界
      // 说成精确值。
      reclaimableSubUnknown: (n: number) =>
        `${n} 项体积未知，合计为下限。`,
      autoExcluded: "自动排除",
      autoExcludedSub: "「禁止删除」条目不会进入计划。"
    },
    blockerPrefix,
    blocker: (path: string) => `${blockerPrefix}${path}`,
    blockerTitle: "已阻止 · ",
    ack: {
      force: (n: number) =>
        `含 ${n} 个有未跟踪文件的工作树；这些文件也会被删除。`,
      // 数量阈值由 state.ts 判断，文案只说明本次规模；体积上限传的是格式化好
      // 的字符串，单位跟着当前语言走。
      count: (n: number) =>
        `本次将处理 ${n} 项。`,
      size: (limit: string) => `单次预计可释放超过 ${limit}`,
      // 计划里存在无法测量的条目时，合计只是下界，上面的体积阈值无法判定：
      // 这条理由独立拦一道，而不是猜。
      unmeasurable: (n: number) =>
        `含 ${n} 项体积未知，合计为下限。`
    },
    checks: {
      title: "执行前检查",
      note: "短横线表示执行前会再检查。",
      recheckNotice:
        "执行前会再次检查；已变化的条目会跳过。",
      pathCheck: (n: number) =>
        `路径检查通过（${n} 项）`,
      whitelistPassed: "保护检查通过",
      whitelistBlocked: (n: number) =>
        `${n} 项受保护，已排除`,
      duplicateCheck: (n: number) =>
        `随工作树删除的产物只计一次（${n} 项）。`,
      // 两条推迟的检查说明删除前会核对什么，避免把「已勾选」理解成「一定删除」。
      identityNotRun: "执行前复查路径与体积",
      activityNotRun: "执行前检查新改动"
    },
    channelA: "永久移除工作树",
    items: (n: number) => `${n} 项`,
    noWorktrees: "没有选中工作树",
    needForce: "有未跟踪文件",
    reviewItem: "需人工确认",
    removable: "可直接移除",
    forceNote: (n: number) =>
      `以下 ${n} 个未跟踪文件也会被删除，且无法恢复：`,
    prunableNote: "工作目录已不在；只清理残留记录，不碰文件。",
    cleanNote: "工作区是干净的，不会丢失任何未保存内容",
    channelB: "将产物目录移入{{trash}}",
    trashHeader: (n: number, bytes: string) => `${n} 项 · ${bytes}`,
    columns: {
      directory: "目录",
      files: "文件数",
      size: "体积",
      recoverability: "可恢复性"
    },
    removedWithWorktree: "随工作树删除（不可恢复）",
    recoverable: "可从{{trash}}恢复",
    noArtifacts: "没有选中产物目录",
    confirmationTitle: "执行前确认",
    confirmationIntro: "本次清理需要你确认：",
    permanenceNote:
      "工作树不能从{{trash}}恢复；构建产物可以。",
    ackConfirm: (n: number) =>
      n > 0
        ? `我已看过上面的列表，包括 ${n} 个工作树中会被删除的未跟踪文件；我明白工作树删除是永久的。`
        : "我已看过上面的列表；我明白工作树删除是永久的。",
    confirmRequired: "必须确认",
    confirmed: "已勾选",
    confirmHint: "勾选后才能执行清理",
    confirmRules:
      "工作树永久删除，分支保留；构建产物移入{{trash}}，可以恢复。",
    typeNameBefore: "请输入项目目录名",
    typeNameAfter: "以继续",
    exportTitle: "计划",
    copy: "复制",
    hideExport: "收起导出",
    exportPlan: "复制计划",
    excludedNote: (n: number) =>
      `${n} 个「禁止删除」条目已被自动排除；回到列表可以查看原因`,
    backToAdjust: "返回调整",
    runCleanup: "执行清理",
    running: "正在执行…",
    runningNote: "请保持应用开启。",
  },
  result: {
    title: "执行结果",
    successTitle: "清理完成",
    successSummary: (worktrees: number, freed: string, artifacts: number, trash: string) =>
      `已移除 ${worktrees} 个工作树，立即释放 ${freed}；将 ${artifacts} 个产物目录移入{{trash}}，清空后可再释放 ${trash}。`,
    partialTitle: "清理完成，但有跳过项",
    partialSummary: (
      worktrees: number,
      freed: string,
      artifacts: number,
      trash: string,
      failed: number
    ) =>
      `已移除 ${worktrees} 个工作树，立即释放 ${freed}；将 ${artifacts} 个产物目录移入{{trash}}，清空后可再释放 ${trash}。有 ${failed} 个已勾选条目未删除，原因见下表。`,
    totalAfterEmptying: (size: string) =>
      `清空{{trash}}后合计可释放 ${size}`,
    topRemaining: "未删除的最大项",
    topHandled: "已处理的最大项",
    openTrash: "打开{{trash}}",
    trashNotice:
      "构建产物可从{{trash}}恢复；工作树不能。",
    noTrashNotice:
      "没有目录进入{{trash}}；原因见下表。",
    worktreePermanentNotice: (n: number) =>
      `${n} 个工作树已永久删除，分支仍保留；工作目录和未跟踪文件不会保留。`,
    openLog: "打开日志",
    logFailedTitle: "操作日志没能写入",
    stats: {
      freedNow: "已立即释放",
      freedNowSub: "工作树永久删除",
      freedNowSubUnknown: (n: number) =>
        `至少为已测得的体积 · ${n} 个工作树无法测量体积`,
      inTrash: "在{{trash}}里",
      inTrashSub: (n: number) => `${n} 个产物目录；清空后释放`,
      inTrashSubUnknown: (n: number) =>
        `至少为已测得的体积 · ${n} 个产物目录无法测量体积`,
      handled: "已处理",
      handledSub: (worktrees: number, artifacts: number) =>
        `${worktrees} 个工作树、${artifacts} 个产物目录`,
      skippedFailed: "未删除",
      skippedFailedSub: "都没删，原因见下表"
    },
    perItemTitle: "逐项结果",
    columns: {
      action: "操作",
      target: "目标",
      detail: "说明",
      result: "结果",
      size: "体积"
    },
    empty: "没有执行记录",
    footer:
      "失败的条目不影响其他条目。",
    complete: "清理完成；重新扫描即可确认结果",
    viewHistory: "查看历史记录",
    backToOverview: "返回总览"
  },
  history: {
    title: "历史记录",
    lede: "日志只存在本机，从不上传；存放位置见「设置」",
    copyLog: "复制日志",
    filterAll: (n: number) => `全部 ${n}`,
    filterWorktreeRemove: "移除工作树",
    filterTrashMove: "移入{{trash}}",
    empty: "没有记录",
    reason: {
      confirmed: "经用户确认后执行",
      rebuildable: "可重建的产物",
      metadata: "清理残留记录"
    }
  },
  settings: {
    title: "设置",
    scanScopeTitle: "扫描范围",
    extraScanPaths: "额外扫描路径",
    extraScanPathsHint: "每行一条；请使用绝对路径，例如 {{path}}",
    maxDepth: "最大扫描深度",
    maxDepthHint: "越深越慢。",
    perDirTimeout: "单个目录时间上限",
    perDirTimeoutHint: "超时的条目需要人工确认。",
    totalBudget: "扫描总时长上限",
    totalBudgetHint: "到时停止；未扫描的部分需要确认。",
    thresholdsTitle: "判定条件",
    activityWindow: "闲置天数",
    activityWindowHint:
      "在这段时间内改动过的条目需要确认。",
    artifactThreshold: "产物显示下限",
    artifactThresholdHint: "更小的产物默认隐藏。",
    untrackedUnsafe: "把未跟踪文件视为不安全",
    untrackedUnsafeHint: "开启后，含未跟踪文件的工作树标记为「禁止删除」",
    protectionTitle: "保护规则",
    whitelistPaths: "白名单路径",
    whitelistPathsHint: "命中的目录不会被删除。",
    protectedBranches: "受保护分支",
    protectedBranchesHint: "逗号分隔；默认分支自动加入。",
    deletionTitle: "删除方式",
    directoryDeletion: "产物目录删除",
    directoryDeletionHint: "产物目录始终移入{{trash}}。",
    directoryDeletionOption: "{{trash}}",
    worktreeDeletion: "工作树删除",
    worktreeDeletionHint:
      "永久删除工作目录，保留分支。",
    worktreeDeletionOption: "永久删除",
    strongProtection: "强保护",
    strongProtectionHint: "清理前必须输入项目目录名。",
    logsTitle: "日志、语言与外观",
    operationLog: "操作日志",
    operationLogHint: (n: number) => `只写在本机，从不上传；目前已有 ${n} 条记录`,
    operationLogUnknown: "当前平台没有日志目录。",
    openDirectory: "打开目录",
    clearLog: "清空日志",
    clearLogTitle: "清空日志内容，文件本身保留",
    language: "语言",
    languageHint: "默认跟随系统，也可以固定为某一种语言",
    locale: {
      system: "跟随系统",
      en: "English",
      zhCN: "简体中文"
    },
    appearance: "外观",
    appearanceHint: "「跟随系统」会自动跟随操作系统的浅色与深色模式",
    system: "跟随系统",
    light: "浅色",
    dark: "深色",
    pathAbbrevBefore: "路径以缩写形式显示，例如",
    pathAbbrevAfter: "；写出的始终是完整绝对路径"
  },
  update: {
    title: "更新",
    status: "状态",
    currentVersion: "当前版本",
    hint: "应用启动时会检查是否有新版本；这次请求不携带任何关于你或你的文件的信息",
    check: "检查更新",
    checking: "正在检查更新…",
    upToDate: "已经是最新版本",
    available: (version: string) => `有新版本 ${version}`,
    download: "下载并安装",
    downloading: (percent: number) => `正在下载… ${percent}%`,
    downloadingUnknown: "正在下载…",
    ready: (version: string) => `${version} 已安装，重启后生效`,
    readyWindows: "新版本已安装；安装程序关闭了应用，重新打开即可使用",
    restart: "立即重启",
    failed: (message: string) => `检查失败：${message}`,
    installFailed: (message: string) => `安装失败：${message}`,
    banner: (version: string) => `可更新至 ${version}`,
    desktopOnly: "只有桌面应用会检查更新"
  }
};
