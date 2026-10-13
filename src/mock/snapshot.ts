import type {
  ArtifactRecord,
  HistoryEntry,
  ProjectSnapshot,
  Reason,
  Settings,
  WorktreeRecord
} from "../types";
import { assess } from "../types";

const DAY = 86400_000;

function daysAgo(days: number): string {
  return new Date(Date.now() - days * DAY).toISOString();
}

export const mockRoot = "/Users/andy/MySrc/remote-mic-app";

export const mockSnapshot: ProjectSnapshot = {
  root: mockRoot,
  scanned_at: new Date().toISOString(),
  default_branch: "main",
  warnings: [
    "worktree cursor/perf-audio 下的 .nuxt 目录扫描超时，体积记为未知并按「需要确认」处理。"
  ],
  worktrees: [
    {
      path: mockRoot,
      branch: "main",
      head: "7c41d9a",
      is_main: true,
      is_detached: false,
      is_locked: false,
      is_prunable: false,
      dirty_files: 0,
      untracked_files: 0,
      untracked_file_list: [],
      ahead: 0,
      behind: 0,
      has_upstream: true,
      merged_into_default: true,
      stash_count: 0,
      last_updated_at: daysAgo(0),
      agent: null,
      size_bytes: 486_000_000,
      safety: assess([
        { code: "wt.main", severity: "Unsafe", text: "这是主工作区", evidence: "git worktree list 首项" },
        { code: "wt.clean", severity: "Safe", text: "工作区干净", evidence: "无未提交改动" }
      ])
    },
    {
      path: `${mockRoot}--codex-fix-ble`,
      branch: "codex/fix-ble-reconnect",
      head: "b83e210",
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
      merged_into_default: true,
      stash_count: 0,
      last_updated_at: daysAgo(26),
      agent: "codex",
      size_bytes: 1_900_000_000,
      safety: assess([
        { code: "wt.clean", severity: "Safe", text: "工作区干净", evidence: "无未提交改动、无未跟踪文件" },
        { code: "wt.merged", severity: "Safe", text: "已合入 main", evidence: "merge-base 判定为祖先" }
      ])
    },
    {
      path: `${mockRoot}--claude-ui`,
      branch: "claude/ui-refresh",
      head: "41ab7c3",
      is_main: false,
      is_detached: false,
      is_locked: false,
      is_prunable: false,
      dirty_files: 3,
      untracked_files: 2,
      untracked_file_list: ["src/components/AudioMeter.vue.bak", "notes/scratch.md"],
      ahead: 2,
      behind: 0,
      has_upstream: true,
      merged_into_default: false,
      stash_count: 0,
      last_updated_at: daysAgo(1),
      agent: "claude",
      size_bytes: 1_100_000_000,
      safety: assess([
        { code: "wt.dirty", severity: "Unsafe", text: "有未提交改动", evidence: "3 个文件有未提交改动" },
        { code: "wt.unpushed", severity: "Unsafe", text: "有未推送提交", evidence: "2 个提交未推送" },
        { code: "wt.unmerged", severity: "Review", text: "未合入 main", evidence: "merge-base 判定不是祖先" }
      ])
    },
    {
      path: `${mockRoot}--cursor-perf`,
      branch: "cursor/perf-audio",
      head: "9de04f1",
      is_main: false,
      is_detached: false,
      is_locked: false,
      is_prunable: false,
      dirty_files: 0,
      untracked_files: 0,
      untracked_file_list: [],
      ahead: 0,
      behind: 0,
      has_upstream: false,
      merged_into_default: false,
      stash_count: 1,
      last_updated_at: daysAgo(2),
      agent: "cursor",
      size_bytes: 2_400_000_000,
      safety: assess([
        { code: "wt.unmerged", severity: "Review", text: "未合入 main", evidence: "merge-base 判定不是祖先" },
        { code: "wt.stash", severity: "Review", text: "有 stash 记录", evidence: "1 条 stash" },
        { code: "wt.recent_activity", severity: "Review", text: "近 7 天仍在改动", evidence: "最后活动：2 天前" }
      ])
    },
    {
      path: `${mockRoot}--review-locked`,
      branch: "review/audio-regression",
      head: "2f8b5c0",
      is_main: false,
      is_detached: false,
      is_locked: true,
      is_prunable: false,
      dirty_files: 0,
      untracked_files: 4,
      untracked_file_list: [
        "fixtures/loop-48k.wav",
        "fixtures/loop-44k.wav",
        "fixtures/noise.wav",
        "scripts/repro.sh"
      ],
      ahead: 0,
      behind: 1,
      has_upstream: true,
      merged_into_default: false,
      stash_count: 0,
      last_updated_at: daysAgo(11),
      agent: null,
      size_bytes: 640_000_000,
      safety: assess([
        { code: "wt.locked", severity: "Unsafe", text: "被 Git 锁定", evidence: "porcelain 标记 locked" },
        { code: "wt.untracked", severity: "Review", text: "有未跟踪文件", evidence: "4 个未跟踪文件" }
      ])
    },
    {
      path: `${mockRoot}--wt-gone`,
      branch: "spike/old-encoder",
      head: "1a7f002",
      is_main: false,
      is_detached: false,
      is_locked: false,
      is_prunable: true,
      dirty_files: 0,
      untracked_files: 0,
      untracked_file_list: [],
      ahead: 0,
      behind: 0,
      has_upstream: true,
      merged_into_default: true,
      stash_count: 0,
      last_updated_at: daysAgo(58),
      agent: null,
      size_bytes: 0,
      safety: assess([
        {
          code: "wt.prunable",
          severity: "Safe",
          text: "工作目录已不存在",
          evidence: "仅清理 .git/worktrees 元数据"
        }
      ])
    }
  ] as WorktreeRecord[],
  artifacts: [
    {
      path: `${mockRoot}/node_modules`,
      kind: "node_modules",
      ecosystem: "node",
      project_root: mockRoot,
      worktree_path: mockRoot,
      size_bytes: 412_000_000,
      file_count: 18240,
      last_updated_at: daysAgo(3),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "可通过 npm install 重建", evidence: "package-lock.json 存在" },
        { code: "af.git_ignored", severity: "Safe", text: "已被 .gitignore 忽略", evidence: ".gitignore:12" },
        { code: "af.recent_activity", severity: "Review", text: "近 7 天有改动", evidence: "最后活动：3 天前" }
      ])
    },
    {
      path: `${mockRoot}/.venv`,
      kind: ".venv",
      ecosystem: "python",
      project_root: mockRoot,
      worktree_path: mockRoot,
      size_bytes: 214_000_000,
      file_count: 9860,
      last_updated_at: daysAgo(44),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "可通过 uv sync 重建", evidence: "pyproject.toml 存在" },
        { code: "af.git_ignored", severity: "Safe", text: "已被 .gitignore 忽略", evidence: ".gitignore:24" }
      ])
    },
    {
      path: `${mockRoot}/dist`,
      kind: "dist",
      ecosystem: "node",
      project_root: mockRoot,
      worktree_path: mockRoot,
      size_bytes: 48_000_000,
      file_count: 210,
      last_updated_at: daysAgo(31),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "可通过 npm run build 重建", evidence: "scripts.build 存在" },
        { code: "af.git_ignored", severity: "Safe", text: "已被 .gitignore 忽略", evidence: ".gitignore:6" }
      ])
    },
    {
      path: `${mockRoot}/.turbo`,
      kind: ".turbo",
      ecosystem: "node",
      project_root: mockRoot,
      worktree_path: mockRoot,
      size_bytes: 96_000_000,
      file_count: 1204,
      last_updated_at: daysAgo(61),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "构建缓存，可重新生成", evidence: "turbo.json 存在" },
        { code: "af.git_ignored", severity: "Safe", text: "已被 .gitignore 忽略", evidence: ".gitignore:9" }
      ])
    },
    {
      path: `${mockRoot}/crates/remote-mic-core/build`,
      kind: "build",
      ecosystem: "rust",
      project_root: mockRoot,
      worktree_path: mockRoot,
      size_bytes: 6_200_000,
      file_count: 23,
      last_updated_at: daysAgo(70),
      git_ignored: false,
      git_tracked: true,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        {
          code: "af.git_tracked",
          severity: "Unsafe",
          text: "目录内有已提交进仓库的文件",
          evidence: "git ls-files 命中 23 个文件，不是纯构建产物"
        }
      ])
    },
    {
      path: `${mockRoot}/apps/desktop/node_modules`,
      kind: "node_modules",
      ecosystem: "node",
      project_root: mockRoot,
      worktree_path: mockRoot,
      size_bytes: null,
      file_count: 0,
      last_updated_at: daysAgo(12),
      git_ignored: true,
      git_tracked: false,
      is_symlink: true,
      symlink_target: "/Users/andy/toolchains/pnpm-store/node_modules",
      safety: assess([
        {
          code: "af.symlink_outside",
          severity: "Unsafe",
          text: "是指向项目外的符号链接",
          evidence: "目标 /Users/andy/toolchains/pnpm-store/node_modules 在项目根之外，删除会误伤共享目录"
        }
      ])
    },
    {
      path: `${mockRoot}--codex-fix-ble/target`,
      kind: "target",
      ecosystem: "rust",
      project_root: `${mockRoot}--codex-fix-ble`,
      worktree_path: `${mockRoot}--codex-fix-ble`,
      size_bytes: 3_100_000_000,
      file_count: 24880,
      last_updated_at: daysAgo(22),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "可通过 cargo build 重建", evidence: "Cargo.toml 存在" },
        { code: "af.git_ignored", severity: "Safe", text: "已被 .gitignore 忽略", evidence: ".gitignore:1" }
      ])
    },
    {
      path: `${mockRoot}--codex-fix-ble/dist`,
      kind: "dist",
      ecosystem: "node",
      project_root: `${mockRoot}--codex-fix-ble`,
      worktree_path: `${mockRoot}--codex-fix-ble`,
      size_bytes: 62_000_000,
      file_count: 264,
      last_updated_at: daysAgo(25),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "可通过 npm run build 重建", evidence: "scripts.build 存在" },
        { code: "af.git_ignored", severity: "Safe", text: "已被 .gitignore 忽略", evidence: ".gitignore:6" }
      ])
    },
    {
      path: `${mockRoot}--claude-ui/node_modules`,
      kind: "node_modules",
      ecosystem: "node",
      project_root: `${mockRoot}--claude-ui`,
      worktree_path: `${mockRoot}--claude-ui`,
      size_bytes: 388_000_000,
      file_count: 17910,
      last_updated_at: daysAgo(1),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "可通过 npm install 重建", evidence: "package-lock.json 存在" },
        { code: "af.git_ignored", severity: "Safe", text: "已被 .gitignore 忽略", evidence: ".gitignore:12" },
        { code: "af.recent_activity", severity: "Review", text: "近 7 天有改动", evidence: "最后活动：1 天前" }
      ])
    },
    {
      path: `${mockRoot}--claude-ui/.next`,
      kind: ".next",
      ecosystem: "node",
      project_root: `${mockRoot}--claude-ui`,
      worktree_path: `${mockRoot}--claude-ui`,
      size_bytes: 178_000_000,
      file_count: 3410,
      last_updated_at: daysAgo(1),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "可通过 next build 重建", evidence: "next.config.ts 存在" },
        { code: "af.git_ignored", severity: "Safe", text: "已被 .gitignore 忽略", evidence: ".gitignore:8" },
        { code: "af.recent_activity", severity: "Review", text: "近 7 天有改动", evidence: "最后活动：1 天前" }
      ])
    },
    {
      path: `${mockRoot}--cursor-perf/target`,
      kind: "target",
      ecosystem: "rust",
      project_root: `${mockRoot}--cursor-perf`,
      worktree_path: `${mockRoot}--cursor-perf`,
      size_bytes: 2_200_000_000,
      file_count: 21340,
      last_updated_at: daysAgo(2),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "可通过 cargo build 重建", evidence: "Cargo.toml 存在" },
        { code: "af.git_ignored", severity: "Safe", text: "已被 .gitignore 忽略", evidence: ".gitignore:1" },
        { code: "af.recent_activity", severity: "Review", text: "近 7 天有改动", evidence: "最后活动：2 天前" }
      ])
    },
    {
      path: `${mockRoot}--cursor-perf/node_modules`,
      kind: "node_modules",
      ecosystem: "node",
      project_root: `${mockRoot}--cursor-perf`,
      worktree_path: `${mockRoot}--cursor-perf`,
      size_bytes: 401_000_000,
      file_count: 18020,
      last_updated_at: daysAgo(9),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "可通过 npm install 重建", evidence: "package-lock.json 存在" },
        { code: "af.git_ignored", severity: "Safe", text: "已被 .gitignore 忽略", evidence: ".gitignore:12" }
      ])
    },
    {
      path: `${mockRoot}--cursor-perf/coverage`,
      kind: "coverage",
      ecosystem: "node",
      project_root: `${mockRoot}--cursor-perf`,
      worktree_path: `${mockRoot}--cursor-perf`,
      size_bytes: 12_000_000,
      file_count: 84,
      last_updated_at: daysAgo(45),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "可通过测试重新生成", evidence: "vitest --coverage" },
        { code: "af.git_ignored", severity: "Safe", text: "已被 .gitignore 忽略", evidence: ".gitignore:15" }
      ])
    },
    {
      path: `${mockRoot}--cursor-perf/.nuxt`,
      kind: ".nuxt",
      ecosystem: "node",
      project_root: `${mockRoot}--cursor-perf`,
      worktree_path: `${mockRoot}--cursor-perf`,
      size_bytes: null,
      file_count: 0,
      last_updated_at: null,
      git_ignored: true,
      git_tracked: null,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        {
          code: "af.cannot_verify",
          severity: "Review",
          text: "无法完成检查",
          evidence: "体积统计超时（30s），无法确认是否含已提交文件"
        }
      ])
    }
  ] as ArtifactRecord[]
};

export const mockHistory: HistoryEntry[] = [
  {
    at: daysAgo(4),
    action: "trash-move",
    target: `${mockRoot}--codex-fix-ble/target/debug/incremental`,
    reason: "命中可重建产物类型 · 超过 30 天未改动",
    result: "ok",
    size_bytes: 812_000_000,
    detail: "移入废纸篓，可恢复"
  },
  {
    at: daysAgo(4),
    action: "worktree-remove",
    target: `${mockRoot}--codex-old-spike`,
    reason: "工作区干净 · 已合入 main",
    result: "ok",
    size_bytes: 1_240_000_000,
    detail: "git worktree remove 执行成功，随后 prune 清理元数据"
  },
  {
    at: daysAgo(4),
    action: "trash-move",
    target: `${mockRoot}/apps/desktop/node_modules`,
    reason: "指向项目外的符号链接",
    result: "skipped",
    size_bytes: 0,
    detail: "path.symlink_outside 校验未通过，未执行"
  },
  {
    at: daysAgo(11),
    action: "trash-move",
    target: `${mockRoot}/crates/remote-mic-core/build`,
    reason: "疑似构建产物",
    result: "failed",
    size_bytes: 6_200_000,
    detail: "git ls-files 发现 23 个已提交文件，已中止。请人工确认后再处理"
  }
];

export const defaultSettings: Settings = {
  active_days: 7,
  min_artifact_mb: 10,
  untracked_is_unsafe: false,
  max_depth: 6,
  per_dir_timeout_sec: 30,
  scan_budget_sec: 600,
  extra_scan_paths: ["~/MySrc", "~/dev", "~/Projects"],
  whitelist_paths: ["~/MySrc/remote-mic-app--review-locked"],
  protected_branches: ["main", "release/*"],
  recent_projects: [mockRoot, "/Users/andy/MySrc/private-marketing-toolkit", "/Users/andy/MySrc/worktree-cleaner"],
  theme: "system",
  require_project_name: false
};

export const scanStages = [
  { key: "worktrees", label: "解析 Git worktree", detail: "git worktree list --porcelain" },
  { key: "status", label: "检查各 worktree 状态", detail: "status · merge-base · stash · upstream" },
  { key: "artifacts", label: "扫描构建产物", detail: "按产物类型清单与项目边界遍历" },
  { key: "verify", label: "复核与汇总", detail: "活跃度复核 · 原因归并 · 体积汇总" }
];

export const scanTrace = [
  "git worktree list --porcelain",
  "status --porcelain=v2 --branch @ remote-mic-app--claude-ui",
  "merge-base --is-ancestor 9de04f1 main @ remote-mic-app--cursor-perf",
  "stash list --format=%gd @ remote-mic-app--cursor-perf",
  "扫描 remote-mic-app--codex-fix-ble/target …",
  "扫描 remote-mic-app--cursor-perf/target …",
  "check-ignore remote-mic-app/.turbo",
  "ls-files crates/remote-mic-core/build → 23 个已跟踪文件",
  "readlink apps/desktop/node_modules → /Users/andy/toolchains/pnpm-store/node_modules",
  "活跃度复核：remote-mic-app--cursor-perf/target 最后活动 2 天前"
];
