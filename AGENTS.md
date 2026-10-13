# Worktree Cleaner 开发规则

- UI 使用 Vue 3。
- 应用宿主使用 Tauri 2。
- Git 解析、状态判断和 artifact 扫描位于纯 Rust crate worktree-core。
- 核心扫描不依赖某个 Agent 厂商目录。
- 默认只读扫描。
- 删除 Worktree 使用 git worktree remove。
- 删除目录使用 macOS Trash 或 Windows 回收站。
- 不使用永久删除命令清理用户目录。
- 删除判断必须显示可验证原因。
- macOS 和 Windows 行为分别验证。

## 已冻结的设计

- 产品设计与判定表见 `docs/PRODUCT_PLAN.md`。
- 开发约束与验收标准见 `docs/DEV_CONTRACT.md`，实现必须遵循该文件。
- 涉及 DEV_CONTRACT 中 D1–D16、字段契约或原因码语义的改动，必须先改文档再改代码。
- 界面结构与信息结构以 `src/` 下已冻结的原型为准；生产数据接入后 `src/mock/` 退出生产路径。
