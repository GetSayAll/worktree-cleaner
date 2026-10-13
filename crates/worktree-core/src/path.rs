//! 删除前的路径校验。
//!
//! 任何删除动作都必须先通过 [`validate_path_for_deletion`]（开发契约 D10）。
//! 校验只允许**收紧**：解析符号链接的结果只能让判定更严格，不能因为解析
//! 出来一个「看起来正常」的路径就放行原本被拒的输入。
//!
//! 校验项（对应产品规划 6.4）：
//!
//! 1. 非空且为绝对路径；
//! 2. 路径本身是符号链接时，解析目标后重新校验目标；
//! 3. 路径的**祖先**是符号链接时，按解析后的真实路径重新校验；
//! 4. 拒绝把 `..` 作为完整路径组件出现；
//! 5. 拒绝包含 NUL 与控制字符的路径；
//! 6. 归一化后拒绝系统位置与顶层根目录；
//! 7. 拒绝「拼接结果与预期父目录相同」的空变量塌缩；
//! 8. 命中用户白名单即拒绝。
//!
//! 第 2、3 项的实现放在第 6 项之后（先做纯字符串判定，再解析文件系统），
//! 顺序与文档列表不同但检查集合一致；解析结果同样会跑一遍第 6、7 项。

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

/// 深层禁止删除的系统位置（含整个子树）。
///
/// 注意：`/Applications`、`/Library`、`/Volumes`、`/Users` 这类目录采取
/// 「只禁止删除目录本身」的语义，由第 6 项的「组件数 ≤ 2」规则统一覆盖，
/// 因此不在这里列成子树前缀——否则 `/Volumes/External/repo` 这类合理的
/// 项目路径会被误杀。
#[cfg(unix)]
const DENIED_PREFIXES: &[&str] = &[
    "/System",
    "/bin",
    "/sbin",
    "/etc",
    "/private/etc",
    "/var/db",
    "/Library/Extensions",
    "/dev",
    "/proc",
    "/sys",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathRejection {
    #[error("路径为空")]
    Empty,
    #[error("路径不是绝对路径：{0}")]
    NotAbsolute(String),
    #[error("路径包含 NUL 或控制字符：{0}")]
    ControlCharacter(String),
    #[error("路径包含 .. 组件：{0}")]
    ParentTraversal(String),
    #[error("拒绝删除顶层根目录：{0}")]
    TopLevelRoot(String),
    #[error("指向受保护的系统位置：{0}")]
    ProtectedLocation(String),
    #[error("拼接结果与预期父目录相同，疑似变量为空：{0}")]
    EmptyVariableCollapse(String),
    #[error("符号链接指向受保护位置：{0} -> {1}")]
    SymlinkEscape(String, String),
    #[error("命中用户白名单：{0}")]
    Whitelisted(String),
    #[error("无法解析路径：{0}")]
    Unresolvable(String),
}

/// 校验上下文。
#[derive(Debug, Clone, Default)]
pub struct DeletionContext {
    /// 期望的父目录。候选路径与它相同即判定为空变量塌缩。
    pub expected_parent: Option<PathBuf>,
    /// 用户白名单。规则必须是**已展开的绝对路径**（调用方负责展开 `~`）。
    pub whitelist: Vec<PathBuf>,
}

impl DeletionContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_expected_parent(mut self, parent: impl Into<PathBuf>) -> Self {
        self.expected_parent = Some(parent.into());
        self
    }

    pub fn with_whitelist(mut self, whitelist: Vec<PathBuf>) -> Self {
        self.whitelist = whitelist;
        self
    }
}

/// 通过校验的路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPath {
    /// 实际要被移除的路径（**不跟随**叶子符号链接，移走的永远是链接本身）。
    pub target: PathBuf,
    /// 解析后的真实路径，用于二次复核与证据展示。
    pub resolved: PathBuf,
    /// 叶子是否为符号链接。为真时只允许移动链接本身，禁止递归删除。
    pub is_symlink: bool,
}

impl ValidatedPath {
    pub fn as_path(&self) -> &Path {
        &self.target
    }
}

/// 删除前校验。返回 `Err` 即表示**必须拒绝**该删除动作。
pub fn validate_path_for_deletion(
    candidate: &Path,
    ctx: &DeletionContext,
) -> Result<ValidatedPath, PathRejection> {
    // ---- 1 / 5：非空、绝对、无控制字符 ----
    if candidate.as_os_str().is_empty() {
        return Err(PathRejection::Empty);
    }
    if !candidate.is_absolute() {
        return Err(PathRejection::NotAbsolute(display(candidate)));
    }
    let literal = display(candidate);
    if literal.chars().any(|c| c == '\0' || c.is_control()) {
        return Err(PathRejection::ControlCharacter(escape(&literal)));
    }

    // ---- 4：`..` 作为完整路径组件 ----
    for component in candidate.components() {
        if matches!(component, Component::ParentDir) {
            return Err(PathRejection::ParentTraversal(literal));
        }
    }

    let normalized = lexical_normalize(candidate);

    // ---- 6 / 7（字面量）：顶层根、系统位置、空变量塌缩 ----
    reject_roots(&normalized)?;
    reject_empty_variable_collapse(&normalized, ctx)?;

    // ---- 2 / 3：解析符号链接（叶子 + 祖先）----
    let (resolved, is_symlink) = resolve_for_inspection(candidate)?;

    // ---- 6 / 7（解析结果）：同样的判定在真实路径上再跑一遍，只收紧 ----
    if resolved != normalized {
        if let Err(rejection) = reject_roots(&resolved) {
            return Err(match (rejection, is_symlink) {
                (PathRejection::ProtectedLocation(hit), true) => {
                    PathRejection::SymlinkEscape(literal, hit)
                }
                (PathRejection::TopLevelRoot(hit), true) => {
                    PathRejection::SymlinkEscape(literal, hit)
                }
                (other, _) => other,
            });
        }
        reject_empty_variable_collapse_resolved(&resolved, ctx)?;
    }

    // ---- 8：白名单 ----
    for rule in &ctx.whitelist {
        let rule_normalized = lexical_normalize(rule);
        if normalized == rule_normalized || normalized.starts_with(&rule_normalized) {
            return Err(PathRejection::Whitelisted(display(rule)));
        }
    }

    Ok(ValidatedPath {
        target: normalized,
        resolved,
        is_symlink,
    })
}

/// 顶层根 + 系统位置判定。
fn reject_roots(path: &Path) -> Result<(), PathRejection> {
    // 组件数 ≤ 2 即「/xxx」这一层：任何顶层目录本身都不允许删除，
    // 这条路同时覆盖 / 本身、/etc、/Applications、/Library、/Volumes、
    // /Users、/tmp、/opt 等，且不依赖平台特有的清单。
    if path.components().count() <= 2 {
        return Err(PathRejection::TopLevelRoot(display(path)));
    }

    if let Some(hit) = protected_location(path) {
        return Err(PathRejection::ProtectedLocation(hit));
    }

    Ok(())
}

/// 命中受保护位置时返回命中的规则文本，否则返回 `None`。
fn protected_location(path: &Path) -> Option<String> {
    #[cfg(unix)]
    {
        for prefix in DENIED_PREFIXES {
            let prefix_path = Path::new(prefix);
            if path == prefix_path || path.starts_with(prefix_path) {
                return Some((*prefix).to_string());
            }
        }

        // /usr 整体禁止，但 /usr/local 之下是用户与包管理器安装软件的位置。
        let usr = Path::new("/usr");
        let usr_local = Path::new("/usr/local");
        if path == usr || (path.starts_with(usr) && !path.starts_with(usr_local)) {
            return Some("/usr".to_string());
        }
        if path == usr_local {
            return Some("/usr/local".to_string());
        }
    }

    #[cfg(windows)]
    {
        if let Some(prefix) = path.components().next() {
            if matches!(prefix, Component::Prefix(_)) && path.components().count() <= 1 {
                return Some(display(path));
            }
        }
        for key in ["WINDIR", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"] {
            if let Some(root) = std::env::var_os(key) {
                let root_path = Path::new(&root);
                if path == root_path || path.starts_with(root_path) {
                    return Some(display(root_path));
                }
            }
        }
    }

    // 用户主目录本身不允许删除；其下内容不受此条限制。
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home_path = Path::new(&home);
        if !home_path.as_os_str().is_empty() && path == home_path {
            return Some(display(home_path));
        }
    }

    None
}

fn reject_empty_variable_collapse(path: &Path, ctx: &DeletionContext) -> Result<(), PathRejection> {
    if let Some(parent) = ctx.expected_parent.as_ref() {
        if path == lexical_normalize(parent) {
            return Err(PathRejection::EmptyVariableCollapse(display(path)));
        }
    }
    Ok(())
}

fn reject_empty_variable_collapse_resolved(
    resolved: &Path,
    ctx: &DeletionContext,
) -> Result<(), PathRejection> {
    if let Some(parent) = ctx.expected_parent.as_ref() {
        let parent_resolved =
            resolve_for_inspection(parent).map(|(path, _)| path).unwrap_or_else(|_| lexical_normalize(parent));
        if resolved == parent_resolved {
            return Err(PathRejection::EmptyVariableCollapse(display(resolved)));
        }
    }
    Ok(())
}

/// 词法归一化：去掉 `.`，收敛 `..`，不触碰文件系统。
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(segment) => out.push(segment),
        }
    }
    if out.as_os_str().is_empty() {
        out.push("/");
    }
    out
}

/// 解析路径的真实位置，用于符号链接判定。
///
/// 路径不存在时取「最长的已存在祖先」做归一化，再把剩余组件接回去——
/// 这样即使待删除目录已经被删掉，也能判断它的祖先有没有被符号链接劫持。
fn resolve_for_inspection(path: &Path) -> Result<(PathBuf, bool), PathRejection> {
    let is_symlink = std::fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false);

    let mut current = path.to_path_buf();
    let mut tail: Vec<OsString> = Vec::new();
    loop {
        if let Ok(canonical) = std::fs::canonicalize(&current) {
            let mut out = canonical;
            for segment in tail.iter().rev() {
                out.push(segment);
            }
            return Ok((out, is_symlink));
        }

        let name = match current.file_name() {
            Some(name) => name.to_os_string(),
            None => return Err(PathRejection::Unresolvable(display(path))),
        };
        tail.push(name);

        match current.parent() {
            Some(parent) if parent != current => current = parent.to_path_buf(),
            _ => return Err(PathRejection::Unresolvable(display(path))),
        }
    }
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// 把控制字符渲染成可见形式，便于把证据写进日志和界面。
fn escape(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            '\0' => "\\0".to_string(),
            '\n' => "\\n".to_string(),
            '\t' => "\\t".to_string(),
            '\r' => "\\r".to_string(),
            other if other.is_control() => format!("\\u{{{:x}}}", other as u32),
            other => other.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    /// 建一个只在本次测试内使用的临时目录。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "worktree-core-path-{}-{}-{}",
                tag,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            std::fs::create_dir_all(&dir).expect("创建临时目录");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn assert_rejected(path: &str, ctx: &DeletionContext) {
        let result = validate_path_for_deletion(Path::new(path), ctx);
        assert!(result.is_err(), "应当拒绝但通过了：{}", path);
    }

    #[test]
    fn accepts_an_ordinary_project_path() {
        let ctx = DeletionContext::new();
        let validated = validate_path_for_deletion(
            Path::new("/Users/andy/MySrc/proj/node_modules"),
            &ctx,
        )
        .expect("普通项目路径应当通过");
        assert_eq!(validated.as_path(), Path::new("/Users/andy/MySrc/proj/node_modules"));
        assert!(!validated.is_symlink);
    }

    #[test]
    fn rejects_empty_path() {
        assert_eq!(
            validate_path_for_deletion(Path::new(""), &DeletionContext::new()),
            Err(PathRejection::Empty)
        );
    }

    #[test]
    fn rejects_relative_path() {
        assert_rejected("node_modules", &DeletionContext::new());
        assert_rejected("./build", &DeletionContext::new());
        assert_rejected("../target", &DeletionContext::new());
    }

    #[test]
    fn rejects_control_characters() {
        assert_rejected("/tmp/a\0b", &DeletionContext::new());
        assert_rejected("/tmp/a\tb", &DeletionContext::new());
        assert_rejected("/tmp/a\nb", &DeletionContext::new());
    }

    #[test]
    fn rejects_parent_traversal_components() {
        assert_rejected("/Users/andy/../etc/passwd", &DeletionContext::new());
        assert_rejected("/..", &DeletionContext::new());
        assert_rejected("/Users/andy/proj/..", &DeletionContext::new());
    }

    #[test]
    fn allows_legitimate_names_containing_dots() {
        // 名字里出现 `..` 不是路径穿越，不能一刀切拒绝。
        let ctx = DeletionContext::new();
        assert!(validate_path_for_deletion(Path::new("/tmp/name..files"), &ctx).is_ok());
        assert!(validate_path_for_deletion(Path::new("/tmp/..hidden"), &ctx).is_ok());
    }

    #[test]
    fn rejects_top_level_roots() {
        for path in ["/", "/etc", "/Applications", "/Library", "/Volumes", "/Users", "/tmp", "/opt"] {
            assert_rejected(path, &DeletionContext::new());
        }
    }

    #[test]
    fn rejects_deep_system_locations() {
        for path in [
            "/System/Library/CoreServices",
            "/bin/sh",
            "/sbin/mount",
            "/usr/bin/git",
            "/etc/passwd",
            "/var/db/whatever",
            "/Library/Extensions/foo.kext",
            "/dev/null",
        ] {
            assert_rejected(path, &DeletionContext::new());
        }
    }

    #[test]
    fn allows_usr_local_but_not_usr_local_itself() {
        let ctx = DeletionContext::new();
        assert!(validate_path_for_deletion(Path::new("/usr/local/bin/mytool"), &ctx).is_ok());
        assert_rejected("/usr/local", &ctx);
    }

    #[test]
    fn rejects_home_directory_itself_but_allows_children() {
        let home = match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home),
            None => return,
        };
        let ctx = DeletionContext::new();
        assert!(validate_path_for_deletion(&home, &ctx).is_err(), "主目录本身不能删");
        assert!(validate_path_for_deletion(&home.join("proj/node_modules"), &ctx).is_ok());
    }

    #[test]
    fn rejects_empty_variable_collapse() {
        let ctx = DeletionContext::new().with_expected_parent("/Users/andy/proj");
        // 拼接结果塌缩成父目录本身，说明某个变量是空的。
        assert_eq!(
            validate_path_for_deletion(Path::new("/Users/andy/proj"), &ctx),
            Err(PathRejection::EmptyVariableCollapse("/Users/andy/proj".to_string()))
        );
        // 正常拼接不受影响。
        assert!(validate_path_for_deletion(Path::new("/Users/andy/proj/target"), &ctx).is_ok());
    }

    #[test]
    fn rejects_whitelisted_paths_and_their_children() {
        let ctx = DeletionContext::new()
            .with_whitelist(vec![PathBuf::from("/Users/andy/MySrc/keep-me")]);
        assert_rejected("/Users/andy/MySrc/keep-me", &ctx);
        assert_rejected("/Users/andy/MySrc/keep-me/node_modules", &ctx);
        // 前缀相同但不是同一路径组件的不受影响。
        assert!(validate_path_for_deletion(Path::new("/Users/andy/MySrc/keep-me-other"), &ctx).is_ok());
    }

    #[test]
    #[cfg(unix)]
    fn rejects_symlink_pointing_into_protected_location() {
        let temp = TempDir::new("leaf-link");
        let link = temp.path().join("leaf-link");
        symlink("/System", &link).expect("创建符号链接");

        let result = validate_path_for_deletion(&link, &DeletionContext::new());
        match result {
            Err(PathRejection::SymlinkEscape(from, hit)) => {
                assert!(from.ends_with("leaf-link"), "证据应指向链接本身：{}", from);
                assert!(hit.starts_with("/System"), "证据应指向被保护位置：{}", hit);
            }
            other => panic!("应判定为符号链接逃逸，实际：{:?}", other),
        }
    }

    #[test]
    #[cfg(unix)]
    fn rejects_symlinked_ancestor_pointing_into_protected_location() {
        let temp = TempDir::new("ancestor-link");
        let link = temp.path().join("usr-link");
        symlink("/usr", &link).expect("创建符号链接");

        // 叶子不存在（/usr/bin 存在，所以用更深一层不存在的位置），
        // 但祖先 usr-link 指向 /usr，解析后落在受保护子树里。
        let candidate = link.join("bin/definitely-not-here");
        let result = validate_path_for_deletion(&candidate, &DeletionContext::new());
        assert!(result.is_err(), "祖先符号链接指向 /usr 时必须拒绝，实际：{:?}", result);
    }

    #[test]
    fn root_path_is_resolvable() {
        // 绝对路径总能沿祖先走到 `/`，所以解析失败只可能是防御性分支。
        let (resolved, is_symlink) = resolve_for_inspection(Path::new("/")).expect("根目录可解析");
        assert_eq!(resolved, Path::new("/"));
        assert!(!is_symlink);
    }

    #[test]
    fn resolving_nonexistent_path_keeps_tail_components() {
        let temp = TempDir::new("missing-tail");
        let candidate = temp.path().join("gone/also-gone");
        let (resolved, is_symlink) = resolve_for_inspection(&candidate).expect("可解析到已存在祖先");
        assert!(!is_symlink);
        assert!(resolved.ends_with("gone/also-gone"), "尾部组件应保留：{}", resolved.display());
    }

    #[test]
    fn validates_normalized_target_form() {
        let ctx = DeletionContext::new();
        let validated = validate_path_for_deletion(Path::new("/tmp/./a/./b"), &ctx).expect("通过校验");
        assert_eq!(validated.as_path(), Path::new("/tmp/a/b"));
    }
}
