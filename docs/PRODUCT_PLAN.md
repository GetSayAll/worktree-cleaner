# Worktree Cleaner 产品规划

跨 Agent 的 Git Worktree 与构建产物安全审计工具。默认只读扫描，删除前给出可验证原因，删除走系统废纸篓并同步清理 Git worktree 元数据。

平台：macOS、Windows。技术：Vue 3 + Tauri 2 + Rust（worktree-core）。

---

## 一、产品定位

**一句话**：让开发者敢删自己电脑上那些「不知道是谁建的、不知道能不能删的」worktree 和构建产物。

**要解决的三个真实问题**

1. Codex / Claude Code / Cursor / WorkBuddy / Aider 各自建 worktree，散在仓库旁边，没人记得哪个还有用。`git worktree list` 只能看到路径和 HEAD，看不出「这个能不能删」。
2. 构建产物（`node_modules`、`target`、`dist`）动辄几个 GB，但删错一次就要重新下载或重新编译；有些目录名字像产物、其实已经提交进仓库，删了就丢工作。
3. 现有工具要么是纯命令行（`mo purge` 之类）不给 GUI，要么是「一键清理」把判断权收走——用户看不到删除理由，不敢用。

**核心不变量（贯穿全部功能）**

- 删除判断必须能落到一条可验证的证据上（命令输出、路径、计数），而不是「看起来像垃圾」。
- 证据拿不到就按「未知」处理，未知永远不判为可安全删除。
- 永远不永久删除用户目录内容；一律进系统废纸篓。
- 主工作区永不可删。

**目标用户**：同时用 2 个以上 AI 编码 Agent 的中高级开发者；自己的机器，本机权限足够，不做企业集中管控。

**交付形态**：不做纯 Web 版。交付形态为「只读报告版 → 本地 daemon + 浏览器 → Tauri 桌面外壳」三条，定义见第十节，阶段划分见第八节。

---

## 二、核心概念：三档安全等级

所有 Worktree 和构建产物都归入三档之一，等级由「原因清单」推导，而不是独立打分。

| 等级 | 含义 | UI 表现 | 是否默认可选 |
|---|---|---|---|
| **Safe** 可安全删除 | 全部检查通过，且有正向证据（已合并 / 已被 gitignore） | 绿色徽标 | 是，默认勾选 |
| **Review** 需要确认 | 不构成危险，但存在不确定因素（未合并、近 7 天有活动、读取失败） | 琥珀色徽标 | 是，但默认**不**勾选，需用户主动勾 |
| **Unsafe** 不可删除 | 命中硬性禁止条件（主工作区、有未提交改动、有未推送提交、被 git 锁定、已被跟踪） | 红色徽标 | 否，复选框禁用 |

一条规则：**等级由 `reasons` 里最严重的那条决定**。UI 上任何地方显示等级，旁边必须能展开看到原因。

---

## 三、信息架构与页面

左侧固定导航 + 顶部项目栏。

```
顶栏：项目路径 · 扫描时间 · 三档计数 · [重新扫描]
侧栏：概览 / Worktree / 构建产物 / 清理计划 / 历史 / 设置
```

未扫描时侧栏仅「欢迎」可用；扫描完成后才出现数据类页面。

### 1. 欢迎 / 项目选择

- 路径输入框 + 「选择文件夹」（Tauri dialog 插件）；支持把文件夹拖进窗口。
- 「最近项目」卡片列表：路径、上次扫描时间、上次结果摘要（可清理 N 项 / 可释放 X）、三档圆点。
- 三张能力说明卡：只读扫描、可验证原因、废纸篓删除。
- 首次使用时的默认行为：自动列出 `~/MySrc`、`~/dev`、`~/Projects`、`~/Code` 等常见目录下的仓库，作为「最近项目」的兜底。
- 明确的只读承诺文案：扫描阶段不写任何文件。

### 2. 扫描进度

- 阶段列表（逐项打勾）：解析 Git worktree → 检查各 worktree 状态 → 扫描构建产物 → 汇总。
- 每阶段实时显示当前处理路径（单行滚动），不显示进度条百分比（目录数量事先未知，假进度会骗人）。
- 已用时；「取消」按钮（取消后已完成部分仍可查看）。
- 降级提示区：某个 worktree 检查超时 / 无权限读取时在这里列出来，并说明该条目将被判为「未知」。
- 长耗时阶段的超时策略：单目录探测有超时，整轮扫描有总预算；预算耗尽即停止并如实报告「未扫完」，不静默补全。

### 3. 概览

- 三张大卡：可安全清理（数量 + 可释放空间）、需要确认（数量）、不可删除（数量）。
- 主数字：**预计可释放空间**（Safe + Review 已勾选项合计）。
- 两组摘要列表：Worktree 与构建产物各取体积最大的 5 条，行内含等级徽标与一句主因，点击跳对应页。
- 扫描告警区（若有）。
- 主按钮：「生成清理计划」——未勾选任何项时禁用并说明原因。

### 4. Worktree 列表

表格列：选择框 / 分支 / 状态（干净·脏 N·锁定）/ Agent / 最后活动 / 大小 / 安全等级 / 展开。

展开后的详情面板：

- 完整路径、HEAD 短 SHA、是否 detached。
- 与上游的关系：ahead / behind；无上游时明确写「无上游分支」而不是留空。
- 是否已合并进默认分支（默认分支名显示出来，可改）。
- stash 数量、未跟踪文件数量。
- 推断的占用 Agent（按路径特征识别 `codex` / `claude` / `cursor` / `aider` / `workbuddy`，识别不出显示「未知」，不猜）。
- **原因清单**：每条一行，绿色为正向证据（已合并 / 干净），琥珀、红色为风险项，每条后面跟证据文本。

顶部：搜索框 + 筛选（全部 / 可安全删除 / 需要确认 / 不可删除 / 已锁定 / 未合并）。

底部浮动操作条：`已选 N 项 · 预计释放 X` → 「查看清理计划」。

### 5. 构建产物

- 按所属 worktree / 项目根分组，可折叠；组标题显示该组合计体积。
- 每行：类型徽标（`node_modules` / `target` / `dist` / `.venv` …）、相对路径、大小、文件数、最后活动时间、Git 状态（已忽略 / 未跟踪 / 已跟踪）、安全等级。
- 排序：按体积（默认）/ 按最后活动。
- 体积阈值滑块：默认只显示大于 10 MB 的目录（小目录噪音大）。
- 与 Worktree 的关联：如果某产物所在的 worktree 已被勾选删除，该产物行显示「随 worktree 一并移除」并自动取消独立勾选，避免重复统计体积。

### 6. 清理计划（确认页 —— 产品最重要的一页）

把「要做什么」拆成两条通道分别展示，每条都能落到具体命令或具体路径：

**通道 A：移除 Worktree**

- 每条显示：路径、分支、将执行的命令（`git worktree remove <path>`）。
- 需要 `--force` 的条目（存在未跟踪文件）单独标出，并把**将被强制删除的未跟踪文件清单**逐行列出来——这是用户最需要看到的东西。
- 已丢失的 worktree（目录不存在）走 `git worktree prune`，说明只清理元数据、不动文件。
- 不可删除条目不出现在这里，但页脚显示「已自动排除 N 项不可删除，查看」。

**通道 B：移入废纸篓**

- 每条显示：路径、体积、文件数、是否可恢复（废纸篓可恢复，附「在废纸篓中找回」的说明）。
- 按体积倒序。

**先决条件校验（预演）**

执行前逐项过一遍，不通过就在顶部显示阻断原因，而不是等执行时才报错：

- 路径校验：非空绝对路径、符号链接解析（含祖先）、拒绝 `..` 组件、拒绝系统根目录。
- 白名单命中检查。
- 重复删除检查（worktree 与其内部产物同时被选）。
- 目标仍存在且身份未变（生成计划时记录 inode / 大小，执行前复核；不一致则要求重新扫描）。
- 目标活跃度复核：生成计划时判定「安全」的目录，执行前重新检查是否近 7 天有新写入；若已变活跃则挂起该项并要求用户确认。

**高风险确认**

- 出现 `--force`、或删除项数超过阈值（默认 5）、或单次释放空间超过阈值（默认 10 GB）时，必须手动勾选「我已确认上面列出的内容」。
- 可选强保护（设置项，默认关闭）：需在输入框键入项目目录名才能启用「执行」。

**导出**：把计划导出为纯文本 / JSON，便于先人工核对或用脚本审查。

### 7. 执行结果

- 逐条结果：成功 / 已跳过 / 失败，各带原因。
- 汇总：释放空间、移入废纸篓的目录数、Worktree 元数据清理结果。
- 失败项：给出可复制的原因文本和对应命令，方便手动处理。
- 操作按钮：「打开废纸篓」「查看日志」「重新扫描」。

### 8. 历史

- 时间倒序列表：时间、动作（移除 worktree / 移入废纸篓）、目标路径、原因摘要、结果、体积。
- 筛选：全部 / Worktree / 产物；支持导出。
- 一条固定说明：日志只记录在本机，不上报。

### 9. 设置

- **扫描范围**：额外扫描路径（每行一个）、最大扫描深度、单目录超时、整轮扫描预算。
- **保护规则**：白名单路径（命中的目录无论如何不删）、受保护分支名（默认自动识别默认分支并显示）、需要保护的 Agent 目录。
- **判定阈值**：活跃天数（默认 7）、产物显示体积阈值（默认 10 MB）、「未跟踪文件」是否算不安全（默认算）。
- **删除方式**：废纸篓（默认且不可更改，附说明「本工具不提供永久删除」）。
- 外观：跟随系统 / 浅色 / 深色。

---

## 四、数据契约

前端类型与 Rust 侧 `serde` 输出保持一致：枚举单元变体序列化为 `"Safe" | "Review" | "Unsafe"`，字段用 snake_case。

`crates/worktree-core/src/lib.rs` 现有结构需要补字段，`scan_project` 的签名不变。

```rust
pub enum SafetyStatus { Safe, Review, Unsafe }

pub struct Reason {
    pub code: String,          // 稳定标识，如 "wt.dirty"
    pub severity: SafetyStatus, // 该条原因导向的最严重等级
    pub text: String,          // 人类可读
    pub evidence: Option<String>, // 证据：命令输出摘要 / 计数 / 路径
}

pub struct SafetyAssessment {
    pub status: SafetyStatus,  // 由 reasons 中最严重者决定
    pub reasons: Vec<Reason>,
}

pub struct WorktreeRecord {
    pub path: String,
    pub branch: Option<String>,
    pub head: Option<String>,          // 短 SHA
    pub is_main: bool,
    pub is_detached: bool,
    pub is_locked: bool,
    pub is_prunable: bool,             // 目录已丢失
    pub dirty_files: u32,
    pub untracked_files: u32,
    pub ahead: u32,
    pub behind: u32,
    pub has_upstream: bool,
    pub merged_into_default: Option<bool>, // None = 无法判定
    pub stash_count: u32,
    pub last_updated_at: Option<String>,
    pub agent: Option<String>,
    pub size_bytes: u64,
    pub safety: SafetyAssessment,
}

pub struct ArtifactRecord {
    pub path: String,
    pub kind: String,                  // "node_modules" | "target" | ...
    pub ecosystem: String,             // "node" | "rust" | "python" | ...
    pub project_root: String,
    pub worktree_path: Option<String>, // 归属的 worktree；主工作区为 Some(root)
    pub size_bytes: Option<u64>,       // None = 无法测量（超时/无权限/符号链接）
    pub file_count: u64,
    pub last_updated_at: Option<String>,
    pub git_ignored: Option<bool>,
    pub git_tracked: Option<bool>,
    pub is_symlink: bool,
    pub safety: SafetyAssessment,
}

pub struct ProjectSnapshot {
    pub root: String,
    pub scanned_at: String,
    pub default_branch: Option<String>,
    pub worktrees: Vec<WorktreeRecord>,
    pub artifacts: Vec<ArtifactRecord>,
    pub warnings: Vec<String>,         // 扫描降级说明
}
```

`is_prunable` 与「目录存在」要区分：`is_prunable` 指 Git 元数据存在但工作目录已丢失。

---

## 五、检查逻辑

### 5.1 Worktree 检查

数据来源全部是 Git 只读命令，不解析 `.git` 文件格式：

- `git worktree list --porcelain` —— 路径、HEAD、branch、`locked`、`prunable`、bare；第一条即主工作区。
- `git -C <wt> status --porcelain=v2 --branch` —— 未提交改动数、未跟踪文件数、ahead/behind、上游分支。
- `git -C <wt> merge-base --is-ancestor <wt-head> <default>` —— 是否已合并（退出码 0 为是，1 为否，其它为无法判定）。
- `git -C <wt> stash list --format=%gd` —— stash 数量。
- HEAD 提交时间与目录 mtime 的较大者作为「最后活动」。
- `git -C <wt> ls-files --others --exclude-standard` —— 未跟踪文件清单（`--force` 时展示）。

判定表：

| 原因码 | 条件 | 等级 | 证据文本 |
|---|---|---|---|
| `wt.main` | 是主工作区 | Unsafe | `git worktree list` 首项 |
| `wt.locked` | porcelain 中带 `locked` | Unsafe | 锁定原因（若有） |
| `wt.dirty` | 未提交改动 > 0 | Unsafe | `N 个文件有未提交改动` |
| `wt.unpushed` | ahead > 0 | Unsafe | `N 个提交未推送` |
| `wt.detached` | detached HEAD 且无上游 | Review | `HEAD 处于游离状态` |
| `wt.no_upstream` | 无上游且未合并 | Review | `分支未设置上游，且未合入 main` |
| `wt.unmerged` | 未合并进默认分支 | Review | `未合入 main` |
| `wt.untracked` | 未跟踪文件 > 0 | Review（可配置为 Unsafe） | `N 个未跟踪文件` |
| `wt.stash` | stash > 0 | Review | `N 条 stash 记录` |
| `wt.recent_activity` | 最后活动 < 活跃天数 | Review | `最后活动：3 天前` |
| `wt.agent_active` | 目录内存在 `.git/index.lock` | Review | `存在 index.lock，可能正在被使用` |
| `wt.cannot_verify` | 任一检查超时 / 无权限 / 命令失败 | Review | `无法读取状态：<原因>` |
| `wt.whitelisted` | 命中用户白名单 | Unsafe | `命中白名单：<规则>` |
| `wt.prunable` | 目录已丢失 | Safe | `工作目录已不存在，仅清理元数据` |
| `wt.merged` | 已合并进默认分支 | Safe（正向） | `已合入 main` |
| `wt.clean` | 无改动、无未跟踪 | Safe（正向） | `工作区干净` |

判为 **Safe** 的充要条件：非主工作区、未锁定、无未提交改动、ahead = 0、已合并进默认分支（`merged_into_default == Some(true)`）、stash = 0、无活跃迹象、无 `cannot_verify`。任一不满足即降级。

**不允许把 `merged_into_default == None`（无法判定）当作已合并。**

### 5.2 构建产物检查

产物类型清单按生态分组（可在设置中增删）：

- Node / JS：`node_modules`、`dist`、`build`、`.next`、`.nuxt`、`.output`、`.turbo`、`.parcel-cache`、`.angular`、`.svelte-kit`、`.astro`、`.expo`、`coverage`
- Rust / 系统：`target`、`.build`
- Python：`venv`、`.venv`、`__pycache__`、`.pytest_cache`、`.mypy_cache`、`.ruff_cache`、`.tox`、`.nox`
- JVM：`build`、`.gradle`、`obj`
- 移动端：`DerivedData`、`Pods`、`.cxx`、`.dart_tool`
- 其它：`vendor`、`.zig-cache`、`zig-out`、`.terragrunt-cache`、`bin`（需 .NET 上下文证据）

判定表：

| 原因码 | 条件 | 等级 | 证据文本 |
|---|---|---|---|
| `af.git_tracked` | 目录内存在被 git 跟踪的文件 | Unsafe | `N 个文件已提交进仓库，不是纯产物` |
| `af.nested_git` | 目录内存在嵌套 `.git` | Unsafe | `目录内有独立仓库` |
| `af.authored_content` | 目录内存在 `*-keypair.json` 等凭据文件 | Unsafe | `<文件名>` |
| `af.symlink_outside` | 目录是符号链接，且指向项目根之外 | Unsafe | `指向 <目标路径>` |
| `af.whitelisted` | 命中白名单 | Unsafe | `命中白名单：<规则>` |
| `af.recent_activity` | 近活跃天数内有写入 | Review | `最后活动：3 天前` |
| `af.cannot_verify` | 体积计算或特征扫描超时 / 无权限 | Review | `无法读取：<原因>` |
| `af.empty` | 目录为空或无可删内容 | Review | `目录为空` |
| `af.git_ignored` | 目录整体被 gitignore 忽略 | Safe（正向） | `已被 .gitignore 忽略` |
| `af.rebuildable` | 命中已知可重建产物类型 | Safe（正向） | `可通过 npm install / cargo build 重建` |

判为 **Safe** 的充要条件：命中可重建类型、被 gitignore 忽略或未被跟踪、无被跟踪文件、无嵌套仓库、无凭据文件、不是越界符号链接、近活跃期内无写入、检查完整完成。

**关键反例（必须写进测试）**：一个叫 `build/` 的目录里如果存在被 git 跟踪的文件，绝不能删——有的项目把构建产物提交进仓库，删掉等于丢源码。产物名只是「候选」，不是「判据」。

同时：产物目录本身绝不能被当作「项目根」来展开扫描。把 `node_modules` 当项目根会把包内的 `dist/` 删掉，留下「npm 认为依赖树完整、实际包已损坏」的状态，恢复必须联网重装。这条要在 worktree 边界识别里硬性排除。

### 5.3 失败即降级（fail closed）

扫描中的任何失败都必须导致等级上升，不能导致等级下降：

| 失败情形 | 处理 |
|---|---|
| git 命令超时 / 非零退出 | 该 worktree 加 `wt.cannot_verify`，判 Review |
| 无权限读取目录 | 体积记为未知（显示 `未知` 而不是 `0`），加 `af.cannot_verify` |
| 体积统计超时 | 显示 `≥ X MB（未扫完）`，加 `af.cannot_verify` |
| 判定默认分支失败 | `merged_into_default = None`，所有依赖合并状态的条目判 Review |

---

## 六、清理逻辑

### 6.1 执行顺序与去重

1. **先移除 Worktree，再处理产物。** 被勾选删除的 worktree 内部的产物不再单独执行删除——`git worktree remove` 已经会移除整个目录。
2. 生成计划时即完成去重：凡是属于「将被移除的 worktree」内部的产物，从废纸篓通道里移出，并在 UI 上标注「随 worktree 一并移除」，体积只计一次。
3. 串行执行，不做并发删除；每条独立记录结果，单条失败不影响后续。

### 6.2 Worktree 移除

- 干净且已合并：`git worktree remove <path>`。
- 存在未跟踪文件：`git worktree remove --force <path>`，且未跟踪文件清单必须在确认页逐行展示过；`--force` 只在用户确认后使用。
- 目录已丢失：`git worktree prune`（或 `git worktree remove` 报错后转 prune），只清理 `.git/worktrees/<name>` 元数据。
- 移除后统一跑一次 `git worktree prune` 收尾，清理残留元数据。
- **不调用 `rm -rf`**。移除失败时保留原目录并如实报告失败，不退回暴力删除。

### 6.3 产物删除：系统废纸篓

- macOS 回退链（按顺序尝试，前一档失败才用下一档）：
  1. `/usr/bin/trash`（若有）。
  2. 原子无覆盖重命名到同一卷的废纸篓目录（不跨卷，跨卷时降级到下一档）。
  3. Finder AppleScript 兜底。
  - 每一步 30 秒超时；任一步超时或失败即记录原因并尝试下一档；三档全失败则报失败，不永久删除。
- Windows：调用回收站 API（`SHFileOperation` / `IFileOperation`），不直接 `Remove-Item -Force`。
- 删除前后各做一次路径校验，防止校验与删除之间目标被替换。

### 6.4 路径校验（所有删除动作的统一入口）

任何删除动作前必须依次通过：

1. 路径非空且为绝对路径。
2. 若路径本身是符号链接，解析目标后重新校验目标。
3. 若路径的任一**祖先**是符号链接，解析后重新校验（只用于收紧，不用于放宽）。
4. 拒绝把 `..` 作为完整路径组件出现。
5. 拒绝包含控制字符的路径。
6. 归一化后拒绝系统与顶层根：`/`、`/bin`、`/sbin`、`/usr`（`/usr/local` 除外）、`/System`、`/etc`、`/var/db`、`/Applications`、`/Library`、`/Volumes`、`/Users`、用户主目录本身；Windows 侧对应 `C:\`、`%WINDIR%`、`%ProgramFiles%`、用户目录本身。
7. 拒绝删除后会导致父目录变成变量的空路径（拼接出的路径与预期父目录相同即拒绝）。
8. 白名单检查。

第 6 条同时防住「拼接时变量为空导致路径塌缩成父目录」这类事故。

### 6.5 执行前复核（防时间差）

- 计划中记录每个目标的身份（路径 + 大小 + 修改时间，可选 inode）。
- 执行前复核身份，不一致则跳过该项并要求重新扫描。
- 对「安全」等级的产物，执行前重跑一次活跃度检查；已变活跃则挂起该项、交由用户再次确认。

### 6.6 日志与可追溯

- 每次操作追加一条本机日志：时间、动作、目标路径、原因摘要、执行命令、结果、释放体积、可恢复位置。
- 日志位置随平台：macOS `~/Library/Logs/Worktree Cleaner/operations.log`，Windows `%LOCALAPPDATA%\Worktree Cleaner\logs\`。
- 日志只写本机，不联网、不上报；设置中提供「清空日志」。
- 历史页读这份日志；失败的条目也写入，便于事后排查。

---

## 七、边界与非目标

- **不做永久删除**，不提供绕过废纸篓的开关。
- **不删除主工作区**，即使它看起来干净。
- **不自动执行清理**；扫描与清理永远是两个显式步骤。
- **不清理系统缓存、`~/Library`、下载目录、安装包**——那是另外一个产品。
- **不修改仓库内容**：`git worktree remove` / `prune` 之外不执行写仓库的命令；不自动 `git branch -d`、不自动 push。
- **不判断「这个分支还需不需要」**：只报告可验证的事实（未合并、未推送、多少天没动），把结论留给用户。
- 不做多项目批量清理，一次一个项目。

---

## 八、交付顺序

| 阶段 | 内容 | 完成标志 |
|---|---|---|
| P0 | worktree-core 路径校验器 + 原因码模型 + worktree 检查 | 单元测试覆盖判定表全部 `wt.*` 原因码；模糊测试语料全部拒绝 |
| P1 | 产物扫描 + 边界识别（含「产物不当项目根」的反例测试） | `build/` 含被跟踪文件的用例判 Unsafe |
| P2 | CLI 骨架 + snapshot JSON 契约（`scan --json`） | 输出符合契约并通过 schema 校验；扫描后仓库无任何改动 |
| P3 | 只读报告版网页（形态 A） | 单文件网页无网络可打开；数据与 JSON 逐项一致；页面不存在任何删除入口 |
| P4 | 本地清理执行 + 日志（形态 A 的执行侧） | macOS 真机删除后可在废纸篓放回；`git worktree list` 无残留 |
| P5 | 本地 daemon + 浏览器界面（形态 B） | 只监听 127.0.0.1；无 token 与跨源请求被拒 |
| P6 | 收尾：设置持久化、历史、导出、Tauri 桌面外壳（可选） | 重启后设置保留；历史来自真实操作日志 |

交付形态的定义与各形态的权责见第十节；逐阶段的可执行验收标准见 `docs/DEV_CONTRACT.md` 第 5 节。

UI 原型只覆盖观感与交互，不含 Rust 实现；形态 A 阶段把数据来源从 `src/mock/` 换成用户拖入的 snapshot JSON，形态 B 阶段再换成 HTTP 请求。

---

## 九、界面原型文件清单

原型已按本规划落地为可直接运行的 Vue 3 页面，全部数据来自 `src/mock/snapshot.ts`，不依赖 Tauri IPC，也不触碰 `crates/` 与 `src-tauri/`。

```
src/
  App.vue                    外壳：侧栏导航 + 顶栏项目栏 + 视图切换
  state.ts                   状态与计划生成：视图路由、选择集、清理计划、执行、历史
  types.ts                   与 Rust serde 输出对齐的类型（含 SafetyStatus / Reason / 记录结构）
  format.ts                  体积、时间、路径展示
  styles.css                 设计令牌与组件样式（含浅色 / 深色两套）
  mock/snapshot.ts           示例数据：6 个 Worktree、14 个产物、历史记录、设置默认值
  components/
    SafetyBadge.vue          三档安全等级徽标
    ReasonList.vue           原因清单（按严重度排序，带证据行）
    StatCard.vue             统计卡片
  views/
    WelcomeView.vue          欢迎 / 项目选择
    ScanningView.vue         扫描进度与降级提示
    OverviewView.vue         概览
    WorktreesView.vue        Worktree 列表与展开详情
    ArtifactsView.vue        构建产物（按 worktree 分组、体积阈值、排序）
    CleanupPlanView.vue      清理计划与确认（通道 A / 通道 B / 先决条件 / 导出）
    ResultView.vue           执行结果
    HistoryView.vue          历史日志
    SettingsView.vue         设置
```

示例数据刻意覆盖了规划里全部判定分支：主工作区、已合并的干净 worktree、有未提交改动的 worktree、未合并且近 7 天有活动的 worktree、被锁定的 worktree、目录已丢失的 worktree、被 Git 跟踪的 `build/`、指向项目外的符号链接、扫描超时的 `.nuxt`。

本地预览三种方式：

1. **零依赖单文件**：`dist/preview.html`，CSS 与 JS 已内联，双击即可在浏览器打开，不需要任何服务。
2. **开发模式**：`pnpm install` 后执行 `pnpm dev`，默认地址 `http://localhost:2430`，带热更新。
3. **常规构建**：`pnpm build`（产物在 `dist/`，需通过 HTTP 访问，直接双击会因 file:// 的 CORS 限制加载不到资源）。

`dist/` 是构建输出目录，单文件版由构建产物内联生成，随源码改动需要重新生成（`pnpm build:preview`）。

---

## 十、交付形态与路线

### 10.1 结论

核心功能无法在纯浏览器中完成，因此不做「纯 Web 版」。交付形态定为三条，按顺序推进：

| 形态 | 说明 | 推进顺序 |
|---|---|---|
| **A · 只读报告版** | 本地 CLI 产出 `snapshot.json`，静态网页拖入查看与决策；网页永不删除任何文件 | 先行 |
| **B · 本地 daemon + 浏览器** | 本地进程监听 `127.0.0.1` 暴露接口，同一个网页接上 HTTP 后具备执行能力 | A 完成后 |
| **C · Tauri 桌面外壳** | 双击即用的桌面外壳，内部复用同一套 core 与服务 | 可选，最后 |

**不做**：托管 SaaS 形态。把本地仓库路径、分支名、未推送提交信息传到云端，与「日志只写本机、不联网」以及产品定位直接冲突，而且删除动作无论如何都必须发生在本地。

### 10.2 为什么纯浏览器不可行

1. **不能执行 `git`**：拿不到 worktree 列表、合并状态、未推送提交数——判定表依赖的基础数据全部缺失，产品会退化成「按目录名删」，正是本产品明确否定的做法。
2. **不能写入系统废纸篓**：浏览器没有任何回收站 API，直接违反「目录删除只走废纸篓」。
3. **`File System Access API` 不构成解法**：仅 Chromium 系支持；`removeEntry()` 是永久删除、没有回收站；要拿到 worktree 信息就得用 JS 重写 git 读取（解析 `.git`、loose objects、packfile），既成本极高又违反「不解析 `.git` 内部格式」；且目录句柄每次会话都需要用户重新授权。
4. **网页无法启动本地进程**，因此也无法自行拉起本地服务。

### 10.3 形态 A · 只读报告版

- **数据来源**：`worktree-cleaner scan --json <path>` 输出的 `snapshot.json`。
- **网页能力**：展示三档等级与原因、筛选、排序、体积阈值、导出清理计划 JSON。**不包含任何删除或写入能力。**
- **与界面原型的差异**：概览页与计划页的「执行清理」替换为「导出计划」；设置页中与执行相关的项（强保护、确认阈值）只作为计划导出参数，不作为可执行开关。
- **交付形态**：单文件 HTML（资源内联），无网络、无服务也能打开。数据全部由用户自己提供，因此可以直接托管为公开页面而不泄露任何信息。
- **典型用途**：清理前人工复核；把审计结果发给别人看；CI 中产出 snapshot 作为构建产物。

### 10.4 形态 B · 本地 daemon + 浏览器

- 本地进程监听 `127.0.0.1`，暴露 `scan` / `plan` / `execute` 三个接口；浏览器打开本机地址即可使用同一套界面。
- **复用**：`worktree-core` 一行不改；形态 A 的网页只把数据来源从「拖入 JSON」换成 HTTP 请求。
- **安全要求**（必须全部满足，见 `DEV_CONTRACT.md` 的 D19）：只绑 `127.0.0.1`；每次启动生成随机 token；严格校验 `Origin` 与 `Host` 以防 DNS rebinding；不使用 CORS 通配符；破坏性接口除 token 外仍需界面二次确认。
- **代价**：用户需要自己启动一次进程（或配置开机自启）。这是相对形态 C 唯一的体验损失。

### 10.5 snapshot JSON 契约

`snapshot.json` 是 CLI 与网页之间的唯一接口，必须带版本号：

```json
{
  "schema_version": 1,
  "root": "/Users/andy/MySrc/remote-mic-app",
  "scanned_at": "2026-10-11T09:12:00Z",
  "default_branch": "main",
  "worktrees": [],
  "artifacts": [],
  "warnings": []
}
```

- 网页能读等于或低于自身支持的 `schema_version`；遇到更高版本时必须明确提示「查看器需要升级」，不允许猜测字段。
- 其余字段与 `src/types.ts` 的 `ProjectSnapshot` 一一对应。

