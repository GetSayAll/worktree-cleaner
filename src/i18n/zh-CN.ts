// 简体中文消息表 —— Worktree Cleaner 的一个译文。
//
// 键名与 en.ts 完全一致，文件名与键名都用英文（开发契约 D23）：语言规则只
// 放行 src/i18n/ 下的 locale 数据，而「键集合必须与 en.ts 相同」由
// i18n/locale.test.ts 断言。新增或改动文案时，两个表必须同一次改完。
//
// 这里只放界面文案。扫描器产出的 reason text / evidence 是数据，不是界面
// 文案，仍随扫描结果留在 src/mock/snapshot.ts 与 Rust 侧。

// 与 en.ts 同名同义：用于生成 plan.blocker()，并让前端能把前缀剥回去。
const blockerPrefix = "被白名单保护规则拦截：";

export const zhCN = {
  // 随运行机器变化的词。字符串里写 `{{trash}}`，由 `applyLocale` 填入本机该用的
  // 那个词（见 `./index.ts`），所以界面上只出现读者所在的那一个系统，不并列两个。
  // `other` 是第三方系统上的浏览器预览：用通用说法，不猜。
  platform: {
    trash: { macos: "废纸篓", windows: "回收站", other: "系统回收站" }
  },
  app: {
    name: "Worktree Cleaner",
    // 暂时留空，等口号定下来。键先留着：文案落地时只需改这一处，别的地方
    // （侧栏布局、两表一致性测试）都不用动。
    tagline: "",
    readOnlyNote:
      "扫描阶段只运行只读 Git 命令；删除一律进系统{{trash}}，不会永久删除。",
    versionNote: "v0.1.0",
    sampleData: "示例数据",
    previewNote: "浏览器预览",
    switchProject: "切换项目",
    noProject: "尚未选择项目",
    pickRepository: "选择一个仓库开始扫描",
    desktopOnly: "这个操作需要桌面应用；浏览器预览访问不到文件系统。"
  },
  preview: {
    notice:
      "浏览器预览 —— 这个版本访问不到文件系统，因此清理操作不可用，页面上的数字都来自示例数据。"
  },
  nav: {
    project: "项目",
    scan: "扫描",
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
      detached: "HEAD 处于游离状态",
      no_upstream: "没有上游分支，且尚未合并",
      unmerged: "未合并进默认分支",
      untracked: "含有未跟踪文件",
      // 扫描不再产生：stash 属于仓库而不属于被判定 worktree。保留只为让旧审计日志仍能渲染。
      stash: "存在 stash 条目",
      recent_activity: "活动时间窗口内有过提交",
      agent_active: "可能有 Git 命令正在运行",
      cannot_verify: "无法完成检查",
      whitelisted: "命中白名单规则",
      prunable: "工作目录已不存在",
      merged: "已合并进默认分支",
      absorbed: "所有提交的改动都已在默认分支中",
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
      rebuildable: "可重新生成的目录"
    }
  },
  // 一次执行动作的结果，按 DetailCode（`src/types.ts`）索引。核心决定「发生了什么」
  // 和原始值，句子写在这里 —— 所以这一列跟着界面语言走，而不是永远英文。
  //
  // `{before}` 与 `{after}` 是发生变化的字段的两侧，`{reason}` 是原因码（下面
  // 的 reasons 表认识它），`{error}` 是 Git 或系统给的话。`{field}` 在渲染前
  // 已从 `fields` 表查好并替换掉。
  detail: {
    yes: "是",
    no: "否",
    // 按 family/name 两级存，而不是直接用带点的码，理由与 reasons 表相同：测试
    // 用点号拆路径，键里带点会被读成两段。
    codes: {
      worktree: {
        not_in_scan: "已不在扫描结果中，未执行任何操作",
        changed: "生成计划后 {field} 变了：{before} → {after}；未删除",
        no_longer_safe: "重新检查后不再判定为可安全删除：{reason}",
        removed: "已用 git worktree remove 移除",
        remove_failed: "git worktree remove 失败：{error}",
        pruned: "git worktree prune 清理了残留元数据",
        prune_failed: "git worktree prune 失败：{error}"
      },
      artifact: {
        not_in_scan: "已不在扫描结果中，未执行任何操作",
        not_a_known_kind: "已不是本工具识别的构建产物目录名，未删除",
        changed: "生成计划后 {field} 变了：{before} → {after}；未删除",
        no_longer_safe: "重新检查后不再判定为可安全删除：{reason}",
        vanished: "该目录已不在磁盘上，无需删除",
        unmeasurable: "重新检查时无法测量这个目录，因此没有动它",
        trashed: "已移入{{trash}}",
        trash_failed: "无法移入{{trash}}：{error}"
      },
      prunable: {
        metadata_only: "该目录早已不存在，只清理了 Git 元数据，未触碰任何文件"
      },
      precondition: {
        failed: "该路径未通过删除前检查，未执行"
      },
      bundled: {
        with_worktree: "随所属工作树一起移除，没有单独操作"
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
    legacy: "这条记录写于本工具开始保留原因码之前"
  },
  // 用户做过什么，按 ActivityAction 索引。删除记录在上面自己的段里；这里是
  // 「这台机器上发生过什么」的另一半，也是读者没有别的办法重建的那一半。
  activity: {
    codes: {
      scan: {
        started: "开始扫描",
        finished: "扫描完成：{worktrees} 个工作树、{artifacts} 个产物目录",
        failed: "扫描没有完成：{error}",
        cancelled: "扫描在 {elapsed} 后被取消"
      },
      project: {
        switched: "从 {from} 切换到这个项目"
      },
      plan: {
        generated: "生成了清理计划：{worktrees} 个工作树、{artifacts} 个产物目录",
        exported: "导出了计划"
      },
      settings: {
        changed: "修改了一项设置"
      },
      cleanup: {
        confirmed: "确认执行清理：{worktrees} 个工作树、{artifacts} 个产物目录，其中 {forced} 个需要 --force"
      },
      trash: {
        opened: "打开了系统{{trash}}"
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
    unknownAgent: "未知代理",
    defaultBranch: "默认分支",
    unsafeTooltip: "禁止删除的条目无法勾选",
    mainWorktree: (name: string) => `${name}（主工作树）`,
    rescan: "重新扫描",
    copied: "已复制",
    action: {
      worktreeRemove: "移除工作树",
      trashMove: "移入{{trash}}",
      prune: "清理元数据"
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
    budgetNote:
      "每个目录探测都有超时，整次扫描也有总预算；预算用尽时会如实报告「扫描未完成」，并把受影响的条目标为「需人工确认」。",
    stages: {
      worktrees: {
        label: "解析 Git 工作树",
        detail: "git worktree list --porcelain -z"
      },
      status: {
        label: "检查每个工作树的状态",
        detail: "status · merge-base · stash · upstream"
      },
      artifacts: {
        label: "扫描构建产物",
        detail: "在项目边界内遍历产物目录名清单"
      },
      verify: {
        label: "校验并汇总",
        detail: "活动时间复检 · 原因合并 · 体积汇总"
      }
    }
  },
  welcome: {
    eyebrow: "Worktree Cleaner",
    title: "项目清理审计",
    lede: "扫描一个仓库，列出它的全部 Git 工作树与构建产物，并逐条说明每一项为什么能删、为什么不能删。扫描默认只读；删除一律进系统{{trash}}。",
    pathPlaceholder: "/path/to/your-repo",
    chooseFolder: "选择文件夹",
    chooseFolderTitle: "打开系统文件夹选择器",
    chooseFolderUnavailable: "文件夹选择器只在桌面应用里可用，请直接输入路径",
    pickerFailed: (message: string) => `文件夹选择器没能打开：${message}`,
    startScan: "开始扫描",
    dropHint: "也可以把项目文件夹拖进这个窗口。扫描阶段不写任何文件。",
    scanFailed: (message: string) => `扫描没有运行：${message}`,
    promises: {
      readOnlyTitle: "只读扫描",
      readOnlyBody:
        "检查阶段只运行只读 Git 命令，不修改仓库，也不向目标目录写入任何内容。",
      evidenceTitle: "可核验的理由",
      evidenceBody:
        "每条判断都带证据：具体命令、文件数量或路径。「看着像垃圾」不算理由。",
      trashTitle: "删除走{{trash}}",
      trashBody:
        "目录删除一律进系统{{trash}}；工作树走 git worktree remove。没有任何永久删除路径。"
    },
    recentTitle: "最近的项目",
    recentHint: "点击即可扫描",
    lastScan: (when: string) => `上次扫描 ${when}`,
    neverScanned: "尚未扫描",
    recentEmpty: "还没有项目记录，先选一个开始吧"
  },
  overview: {
    title: "总览",
    subtitle: (root: string, branch: string) => `${root} · 默认分支 ${branch}`,
    viewWorktrees: "查看工作树",
    viewArtifacts: "查看构建产物",
    stats: {
      safeLabel: "可安全删除",
      safeSub: (bytes: string) => `共 ${bytes} · 可删条目默认勾选`,
      // 部分可删条目测量超时：字节数缺失，因此合计只是下界；这里如实报出无法
      // 测量的条数，而不是悄悄丢掉。`bytes` 参数已自带「≥」前缀
      // （formatBytes 的 approximate），所以这句不能重复写。
      safeSubUnknown: (bytes: string, unknown: number) =>
        `合计 ${bytes} · ${unknown} 项无法测量 · 可删条目默认勾选`,
      reviewLabel: "需人工确认",
      reviewSub: "未合并 / 最近有改动 / 检查未完成",
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
      `已选的 ${n} 项无法测量体积，因此上面显示的是下界`,
    selectHint: "在「工作树」或「构建产物」页面勾选；可删条目默认已勾选",
    adjustSelection: "调整选择",
    generatePlan: "生成清理计划"
  },
  worktrees: {
    title: "工作树",
    dataSource:
      "数据来源：git worktree list --porcelain -z · status --porcelain=v2 · merge-base --is-ancestor",
    outsideRoot: "在所选目录之外",
    outsideRootHint:
      "这个工作树在你所选目录之外。它和其他工作树一样会被扫描、判定，也可以删除。",
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
      "工作目录已不存在，只会运行 git worktree prune 清理残留元数据，不触碰任何文件。",
    untrackedLabel: "未跟踪文件（删除此工作树需要 --force）",
    empty: "没有符合当前筛选条件的工作树",
    upstreamGone: "工作目录已消失",
    upstreamNone: "没有上游分支",
    upstreamCounts: (ahead: number, behind: number) => `上游领先 ${ahead} / 落后 ${behind}`,
    mergeMain: "主工作树，不参与合并状态判定",
    mergeUnknown: (branch: string) => `无法判定是否已合并进 ${branch}`,
    mergeYes: (branch: string) => `已合并进 ${branch}`,
    mergeNo: (branch: string) => `未合并进 ${branch}`,
    mergeAbsorbed: (branch: string) => `改动已在 ${branch} 中（压缩合并）`,
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
    hygieneBody: (n: number) => `有 ${n} 条磁盘卫生规则未通过：worktree 数量、缓存或闲置状态会在清理后继续长回来。`,
    hygieneAction: "查看磁盘卫生"
  },
  artifacts: {
    title: "构建产物",
    caption:
      "候选目录名只是提示；最终判断依据是它是否被 Git 跟踪、是否含有手写内容，以及最近活动时间",
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
    gitUnverified: "未核验",
    gitUntracked: "未跟踪",
    // 每一行自带字段名，没有表头可标注。这个前缀就是表头的替身：写「Git 已忽略」
    // 而不是孤零零一个「已忽略」。
    gitLabel: "Git",
    empty: "没有超过当前体积阈值的产物目录",
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
      "对照 docs/DISK_HYGIENE.md 检查本仓库。全程只读：这里不会删除任何东西，任何一条问题也不会改变某一项「是否可删」的判断。",
    metOf: (met: number, total: number) => `${total} 条规则中符合 ${met} 条`,
    stats: {
      violation: "违反的规则",
      advisory: "建议处理",
      unchecked: "未检查",
      met: "符合的规则"
    },
    violationBanner: (rules: number) =>
      `有 ${rules} 条规则被违反。下面每条问题都写明了涉及的路径和处理办法；目前没有改动任何东西。`,
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
          "位于 {expected} 之外。仓库被删除时，放在仓库旁边的工作树不会跟着消失。",
        inside_but_undeclared:
          "在仓库内部，但不在 {expected} 之下，于是 Git 把它当作普通的未跟踪文件。",
      },
      L2: {
        not_checkable: "从一个仓库内部看不到第二个 clone。",
      },
      L3: {
        nested_worktree: "这个工作树位于 {parent} 内部。",
      },
      L4: {
        not_a_slug: "{name} 不是小写、连字符分隔的形式。",
        too_long: "{name} 有 {length} 个字符；上限是 {limit}。",
        no_date_suffix: "{name} 没有以创建日期 YYYYMMDD 结尾。",
        invalid_date: "{name} 结尾的日期并不存在。",
        missing_repo_prefix: "{name} 没有以 {prefix}- 开头，无法追溯到本仓库。",
      },
      L5: {
        missing_entry: "两个声明路径都没有被忽略；.gitignore 里没有 {entry}。",
        no_worktrees_yet:
          "{entry} 还没有被忽略。目前没有工作树；这条要在创建第一个之前就写好。",
        unanchored_pattern:
          "{source}:{line} 的模式 {pattern} 没有前导斜杠，会在任意层级匹配——某个工作树内部跑一次构建，就会悄悄长出属于它自己的一份。",
        local_only:
          "{pattern} 只写在 {source} 里，而那个文件不会提交：别人的 clone 两个路径都没被忽略。",
        elsewhere: "这些路径由 {source} 忽略，而它不在本仓库自己的 .gitignore 里。",
        nested_worktree_dir: "某个工作树内部出现了 .worktree/ 目录；它只应存在于仓库根目录。",
      },
      L6: {
        not_checkable: "git clean 不留日志，扫描看不到一条已经执行过的命令。",
      },
      L7: {
        not_checkable: "编辑器的文件搜索与系统索引都在仓库之外配置。",
      },
      T1: {
        over_cap: "已注册 {count} 个工作树；上限是 {cap} 个。",
        near_cap: "已注册 {count}/{cap} 个。创建下一个之前先移除一个。",
      },
      T2: {
        merged_still_present:
          "{branch} 已经合入主线。移除该工作树是「完成这次改动」的一部分，不是另一件单独的任务。",
      },
      T3: {
        stale: "连续 {days} 天既无新提交，也无未提交改动。",
        stale_with_changes:
          "{days} 天没有提交，且仍有 {dirty} 处改动、{untracked} 个未跟踪文件。只报告，不删除：得先把成果带出去。",
      },
      T4: {
        past_ceiling: "创建于 {days} 天前；任何工作树都不超过 {ceiling} 天。",
        age_unknown: "{count} 个工作树目录读不出创建时间，因此无法核对其存活时长。",
      },
      T5: {
        not_checkable: "会话的退出没有被记录在任何扫描能读到的地方。",
      },
      B1: {
        outside_declared:
          "这个 {kind} 目录在 build-cache/ 之外，因此无法在一个地方审计、限额和删除。",
      },
      B11: {
        untracked_in_status: "git status 把这个 {kind} 目录列为未跟踪。",
        tracked_in_git: "Git 跟踪着这个 {kind} 目录里的文件。",
      },
      B2: {
        no_cache_root:
          "仓库根目录没有 build-cache/，于是每次构建都写到工具自己的默认位置去了。",
        orphan_slot: "build-cache/{slot} 找不到名为 {slot} 的工作树。",
        missing_slots: "有 {count} 个工作树还没有缓存槽位：{slots}。",
        nested_cache_root: "某个工作树内部出现了第二个 build-cache/。",
      },
      B3: {
        not_checkable: "构建共享哪些依赖，由它自己的配置决定。",
      },
      B4: {
        not_checkable: "某处是否把缓存当作输入来读，光读缓存本身判断不了。",
      },
      B5: {
        slot_over_cap: "这个槽位已占 {size}；单个工作树的上限是 {cap_bytes}。",
        slot_unmeasurable: "这个槽位无法测量，因此体积未知。",
      },
      Q1: {
        frozen:
          "可用空间 {percent}（{available} / {total}）。低于 10% 时不得新建任何东西——工作树、缓存条目、发布产物都不行——直到可用空间回到 12% 以上。",
        warning: "可用空间 {percent}（{available} / {total}）。处于 10%–15% 之间，属于告警。",
        space_unknown: "读不到这个卷的可用空间。",
      },
      Q2: {
        over_share: "build-cache 已占 {size}，为卷容量的 {percent}；建议控制在一成以内。",
        share_unknown: "有 {count} 个缓存槽位无法测量，因此这个占比只是下限。",
      },
      Q3: {
        not_checkable: "准入发生在创建之前；扫描只看得到已经存在的东西。",
      },
      X1: {
        entry_without_expiry:
          "第 {line} 行只写了路径，没有规则也没有失效日期；按现在这样写，它什么都豁免不了。",
        expiry_not_a_date: "第 {line} 行把失效日期写成 {expires}；必须是 YYYY-MM-DD。",
        expired: "第 {line} 行的豁免已于 {expires} 失效。",
        dangling: "第 {line} 行豁免的路径已经不存在了。"
      }
    },
    fixes: {
      L1: "迁进去：git worktree move <路径> <仓库>/.worktree/<仓库>-<分支>-<YYYYMMDD>",
      L2: "保留规范 clone，把另一个改成它的工作树。",
      L3: "移除内层工作树；工作树不会拥有另一个工作树。",
      L4: "用 git worktree move 改名，让目录名带上分支与日期。",
      L5: "把两条条目加进 .gitignore，带前导斜杠，并提交。",
      L6: "装上下面的 shell 包装器。这条规则唯一能有的硬拦截就是它。",
      L7: "把两个路径加进编辑器的搜索排除与系统索引的忽略列表。",
      T1: "先移除一个工作树，再创建下一个。",
      T2: "git worktree remove -- <路径>，然后删除 build-cache/<名称>/。",
      T3: "移除它——如果上面还有成果，先把分支推送出去。",
      T4: "推送分支或导出 patch，然后移除。上限没有例外。",
      T5: "在会话结束前移除，或明确交接：点名该工作树并说明下一步动作。",
      B1: "用 WORKTREE_BUILD_CACHE 把构建指向它自己的槽位，然后删掉这个目录。",
      B11: "把它忽略掉，或移到 build-cache/ 下再忽略。",
      B2: "创建该工作树写入的槽位，或删掉没有归属的那个槽位。",
      B3: "已解析的依赖放在 build-cache/shared/ 共享，编译产物按工作树隔离。",
      B4: "确认没有任何东西把它当作输入读取；此后它随时可以删除。",
      B5: "清空这个槽位。缓存任何时候都可以无预警删除（B4）。",
      Q1: "先腾出空间，再创建别的东西。",
      Q2: "先回收某个闲置工作树的槽位，再新建。",
      Q3: "创建任何东西之前，先过 T1、Q1、Q2 这三关。",
      X1: "在这一行补上规则与失效日期，或者把这一行删掉。"
    },
    findings: {
      title: "问题",
      empty: "本次扫描能检查的规则全部符合。",
      evidence: "依据",
      fix: "处理",
      acceptedTitle: "已接受的例外",
      acceptedUntil: (date: string) => `已接受至 ${date}（见 .worktree-keep）`
    },
    setup: {
      title: "在创建第一个工作树之前做好",
      note:
        "四处接线做到位，其余规则才立得住。状态来自上面的审计；下面的片段可以直接粘贴。",
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
        "L2、L6、L7、T5、B3、B4、Q3 会被报成「未检查」：它们各自取决于一个仓库之外的东西——另一个 clone、一条已经执行过的命令、编辑器的索引、会话的退出，或创建当刻做出的决定。C1–C3 属于机器级缓存与发布产物。没有证据的规则，永远不会被说成「符合」。"
    },
    policyRef: "规则编号与 docs/DISK_HYGIENE.md 一致。"
  },
  plan: {
    title: "清理计划",
    lede: "执行前逐项复核；本页只说明将要发生什么，尚未执行任何操作。",
    backToAdjustSelection: "返回调整选择",
    stats: {
      removeWorktree: "移除工作树",
      removeWorktreeSub: "git worktree remove",
      moveToTrash: "移入{{trash}}",
      moveToTrashSub: (bundled: number) => `另有 ${bundled} 项随所属工作树一并移除`,
      reclaimable: "预计可回收",
      reclaimableSub: "依据扫描时测得的体积",
      // 合计里缺少无法测量的条目，所以只是下界；这里把条数摆出来，避免把下界
      // 说成精确值。
      reclaimableSubUnknown: (n: number) =>
        `至少为已测得的体积 · ${n} 项无法测量`,
      autoExcluded: "自动排除",
      autoExcludedSub: "「禁止删除」的条目已排除在计划之外"
    },
    blockerPrefix,
    blocker: (path: string) => `${blockerPrefix}${path}`,
    blockerTitle: "前置条件未满足 · ",
    ack: {
      force: (n: number) =>
        `包含 ${n} 个需要 --force 的工作树，它们的未跟踪文件也会被删除`,
      // 阈值一律由参数传入，不在文案里写死：数字的唯一定义处在 state.ts
      // （ACK_COUNT_THRESHOLD / ACK_SIZE_THRESHOLD_BYTES），并由调用方传入，
      // 用户读到的数字和代码实际判断的数字因此不可能对不上。体积上限传的是
      // 格式化好的字符串，单位跟着当前语言走。
      count: (n: number, threshold: number) =>
        `本次将处理 ${n} 项，超过 ${threshold} 项的确认阈值`,
      size: (limit: string) => `单次释放超过 ${limit}`,
      // 计划里存在无法测量的条目时，合计只是下界，上面的体积阈值无法判定：
      // 这条理由独立拦一道，而不是猜。
      unmeasurable: (n: number, limit: string) =>
        `包含 ${n} 项无法测量体积，合计只是下界，${limit} 阈值无法判定`
    },
    checks: {
      title: "前置条件检查（dry run）",
      note: "标有短横线的检查会在每项删除之前重新运行",
      pathCheck: (n: number) =>
        `路径检查通过：非空绝对路径、软链接已解析、不含 .. 组件、均不指向系统目录（${n} 项）`,
      whitelistPassed: "白名单检查通过：没有任何条目命中保护规则",
      whitelistBlocked: (n: number) => `${n} 项命中白名单，已被拦截`,
      duplicateCheck: (n: number) =>
        `重复删除检查通过：${n} 个产物随所属工作树移除，只计一次`,
      // 每条只说「要看什么」，不再重复「尚未运行」：列表里的短横线已经说明了状态，
      // `note` 已经说明了时机，两条里各再写一遍就是同一句话说三遍。复检本身是
      // 实现细节，读者拿它做不了什么决策。
      identityNotRun: "路径、体积与最近修改时间的复检",
      activityNotRun: "计划生成之后有没有新的写入"
    },
    channelA: "通道 A · 移除工作树",
    items: (n: number) => `${n} 项`,
    noWorktrees: "没有选中工作树。",
    needForce: "需要 --force",
    removable: "可直接移除",
    forceNote: (n: number) =>
      `--force 会连同下列未跟踪文件一起删除（共 ${n} 个）：它们没有被 Git 跟踪，删除后无法从仓库里恢复：`,
    prunableNote: "工作目录已经不在了。只清理 Git 元数据，不触碰任何文件。",
    cleanNote: "工作区是干净的，不会丢失任何未保存内容。",
    channelB: "通道 B · 移入系统{{trash}}",
    trashHeader: (n: number, bytes: string) => `${n} 项 · ${bytes}`,
    columns: {
      directory: "目录",
      files: "文件数",
      size: "体积",
      recoverability: "可恢复性"
    },
    removedWithWorktree: "随工作树移除",
    recoverable: "可从{{trash}}恢复",
    noArtifacts: "没有选中产物目录",
    confirmationTitle: "执行前确认",
    confirmationIntro: "本次操作命中以下高风险条件，需要人工确认：",
    ackConfirm: (n: number) =>
      `我已逐条看过上面的内容，包括 ${n} 个工作树中被 --force 删除的未跟踪文件`,
    typeNameBefore: "请输入项目目录名",
    typeNameAfter: "以继续",
    exportTitle: "计划导出（JSON）",
    copy: "复制",
    hideExport: "收起导出",
    exportPlan: "导出计划",
    excludedNote: (n: number) =>
      `${n} 个「禁止删除」条目已被自动排除；回到列表可以查看原因`,
    backToAdjust: "返回调整",
    runCleanup: "执行清理",
    running: "正在执行…",
    runningNote: "正在逐项复检并删除，每项都要重新验证，所以需要一点时间；请不要关闭。",
  },
  result: {
    title: "执行结果",
    openTrash: "打开{{trash}}",
    trashNotice:
      "目录已移入系统{{trash}}，可以从{{trash}}恢复；工作树是通过 git 移除的。",
    openLog: "打开日志",
    logFailedTitle: "操作日志没能写入",
    logFailedBody: (path: string, error: string) =>
      `上面的条目仍然处理了，但 ${path} 写入失败（${error}），审计记录在这里断了一环。`,
    stats: {
      spaceFreed: "已释放空间",
      spaceFreedSub: "包含{{trash}}移动与工作树移除",
      // 从头就测不出体积的条目对 `freed` 没有贡献；用「至少」表述，比报一个
      // 假合计诚实。
      spaceFreedSubUnknown: (n: number) =>
        `至少为已测得的体积 · ${n} 项无法测量体积`,
      // 「成功」把两件事混在一起，而用户会拿它和回收站里的数量对：
      // 工作树是 git 直接删的，根本不进回收站，于是 4 成功 / 回收站 1 看着像对不上。
      // 现在说清成功数里有什么、什么才会进回收站。
      succeeded: "已处理",
      succeededSub: (trashed: number) =>
        `其中 ${trashed} 个目录进了{{trash}}；工作树由 git 移除，不进{{trash}}`,
      movedToTrash: "进了{{trash}}",
      movedToTrashSub: "可从{{trash}}恢复",
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
      "失败的条目不影响其他条目；每个动作都会写入本机操作日志。删除不是永久的，目录类目标可以从{{trash}}恢复。",
    complete: "清理完成；重新扫描即可确认结果",
    viewHistory: "查看历史记录",
    backToOverview: "返回总览"
  },
  history: {
    title: "历史记录",
    lede: "日志只存在本机，从不上传。存放位置见「设置」。",
    copyLog: "复制日志",
    filterAll: (n: number) => `全部 ${n}`,
    filterWorktreeRemove: "移除工作树",
    filterTrashMove: "移入{{trash}}",
    empty: "没有记录",
    readFailed: (message: string) => `日志读取失败：${message}`,
    reason: {
      confirmed: "经用户确认后执行",
      rebuildable: "命中可重建的产物类型",
      metadata: "元数据清理"
    }
  },
  settings: {
    title: "设置",
    currentProject: "当前项目：",
    scanScopeTitle: "扫描范围",
    extraScanPaths: "额外扫描路径",
    extraScanPathsHint: "每行一条；留空则使用默认值（~/MySrc、~/dev、~/Projects、~/Code）",
    maxDepth: "最大扫描深度",
    maxDepthHint: "相对扫描根目录计算；越深越慢",
    perDirTimeout: "单目录超时",
    perDirTimeoutHint: "超时后该项按「需人工确认」处理，绝不当作「可安全删除」",
    totalBudget: "扫描总预算",
    totalBudgetHint: "预算用尽即停止，并如实报告未扫描的部分",
    thresholdsTitle: "阈值",
    activityWindow: "活动时间窗口（天）",
    activityWindowHint:
      "只有超过这个天数没有写入，才视为「安静」；否则标记为「需人工确认」",
    artifactThreshold: "产物显示体积阈值",
    artifactThresholdHint: "小于该值的产物默认隐藏，以减少干扰",
    untrackedUnsafe: "把未跟踪文件视为不安全",
    untrackedUnsafeHint: "开启后，含未跟踪文件的工作树标记为「禁止删除」",
    protectionTitle: "保护规则",
    whitelistPaths: "白名单路径",
    whitelistPathsHint: "每行一条；命中的目录永远不会进入清理计划",
    protectedBranches: "受保护分支",
    protectedBranchesHint: "逗号分隔，支持通配符；默认分支会被自动识别并加入",
    deletionTitle: "删除方式",
    directoryDeletion: "目录删除",
    directoryDeletionHint: "固定为系统{{trash}}；本工具不提供永久删除，也没有绕过开关",
    directoryDeletionOption: "系统{{trash}}",
    worktreeDeletion: "工作树删除",
    worktreeDeletionHint:
      "一律走 git worktree remove；目录已消失时使用 git worktree prune",
    worktreeDeletionOption: "git worktree remove（绝不 rm -rf）",
    strongProtection: "强保护",
    strongProtectionHint: "开启后，清理前必须手动输入项目目录名",
    logsTitle: "日志、语言与外观",
    operationLog: "操作日志",
    operationLogHint: (n: number) => `只写在本机，从不上传。目前已有 ${n} 条记录。`,
    operationLogUnknown: "当前平台没有可报告的日志目录。",
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
    pathAbbrevAfter: "；写出的始终是完整绝对路径。"
  }
};
