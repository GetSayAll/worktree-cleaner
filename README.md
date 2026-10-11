# Worktree Cleaner

跨 Agent 的 Git Worktree 和构建 artifacts 安全审计工具。

目标平台：macOS、Windows  
技术方案：Vue 3、Tauri 2、Rust workspace  
许可证：GPL-3.0-only

## 目标

- 使用 Git 标准元数据识别指定项目的全部 Worktree；
- 支持 Codex、Claude Code、WorkBuddy、Cursor、Aider 等 Agent；
- 显示分支、状态、最后更新时间和安全删除原因；
- 识别常见 artifacts 目录，显示大小、最后修改时间和安全删除原因；
- 删除前提供可验证的原因，默认只读；
- 删除阶段使用系统废纸篓或回收站，并清理 Git Worktree 元数据。

## 初始状态

当前提交只包含跨平台项目骨架和扫描器接口。没有安装依赖，没有生成构建缓存，没有执行构建或测试。

## 目录结构

    Vue 3 UI
       ↓ Tauri IPC
    Tauri 2 App Host
       ↓
    worktree-core
       ├── Git Worktree 解析
       ├── Git 状态与安全判断
       └── artifacts 目录扫描

## 许可证

代码使用 GPL-3.0-only。详见 LICENSE。
