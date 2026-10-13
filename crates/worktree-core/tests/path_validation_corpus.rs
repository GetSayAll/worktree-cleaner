//! 路径校验器的对抗测试。
//!
//! 两部分：
//! 1. 语料驱动：`tests/fuzz_corpus/dangerous_paths.txt` 里每一条都必须被拒绝；
//! 2. 生成式：用确定性伪随机拼出大量路径，断言不变量——
//!    **任何通过校验的路径必须是绝对路径、不含 NUL、不含 `..` 组件，
//!    且不是顶层根目录。**

use std::path::{Component, Path, PathBuf};

use worktree_core::{validate_path_for_deletion, DeletionContext};

const CORPUS: &str = include_str!("fuzz_corpus/dangerous_paths.txt");

/// 语料条数的下限（P0 验收标准：≥ 60 条）。
const MIN_CORPUS_ENTRIES: usize = 60;

fn corpus_entries() -> Vec<&'static str> {
    CORPUS
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter(|line| !line.trim().is_empty())
        .collect()
}

#[test]
fn corpus_is_large_enough() {
    let entries = corpus_entries();
    assert!(
        entries.len() >= MIN_CORPUS_ENTRIES,
        "对抗语料至少需要 {MIN_CORPUS_ENTRIES} 条，当前 {} 条",
        entries.len()
    );
}

#[test]
fn every_corpus_entry_is_rejected() {
    let ctx = DeletionContext::new();
    let mut accepted: Vec<String> = Vec::new();
    for entry in corpus_entries() {
        if validate_path_for_deletion(Path::new(entry), &ctx).is_ok() {
            accepted.push(entry.to_string());
        }
    }
    assert!(
        accepted.is_empty(),
        "以下语料条目本应被拒绝却通过了校验：{:#?}",
        accepted
    );
}

#[test]
fn corpus_covers_each_required_category() {
    // 语料必须覆盖 P0 验收要求声明过的类别，避免后续被悄悄删空。
    let joined = corpus_entries().join("\n");
    for needle in ["/", "..", "/System", "/etc", "/usr"] {
        assert!(
            joined.contains(needle),
            "语料应当覆盖类别：{needle}"
        );
    }
    assert!(
        corpus_entries().iter().any(|entry| entry.contains('\t')),
        "语料应当包含控制字符用例"
    );
    assert!(
        corpus_entries().iter().any(|entry| !Path::new(entry).is_absolute()),
        "语料应当包含相对路径用例"
    );
}

/// 确定性伪随机数发生器（xorshift64*），避免为一个属性测试引入额外依赖。
struct XorShift(u64);

impl XorShift {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

const TOKENS: &[&str] = &[
    "..", ".", "a", "bb", "etc", "System", "usr", "usr/local", "bin", "tmp", "Users", "andy",
    "MySrc", "node_modules", "target", "dev", "var", "db", "db", "Library", "Extensions",
    "name..files", " ", "\t", "\0", "passwd",
];

fn random_candidate(rng: &mut XorShift) -> PathBuf {
    let segments = 1 + (rng.next() % 6) as usize;
    let mut raw = String::new();
    if !rng.next().is_multiple_of(4) {
        raw.push('/');
    }
    for index in 0..segments {
        if index > 0 {
            raw.push('/');
        }
        raw.push_str(TOKENS[(rng.next() as usize) % TOKENS.len()]);
    }
    PathBuf::from(raw)
}

#[test]
fn accepted_paths_always_satisfy_the_invariant() {
    let ctx = DeletionContext::new();
    let mut rng = XorShift::new(0x1234_5678_9abc_def0);
    let mut accepted = 0usize;

    for _ in 0..20_000 {
        let candidate = random_candidate(&mut rng);
        let Ok(validated) = validate_path_for_deletion(&candidate, &ctx) else {
            continue;
        };
        accepted += 1;

        // 契约：通过校验的原始路径必须是绝对路径、不含 NUL、不含 `..` 组件。
        assert!(candidate.is_absolute(), "放行了相对路径：{:?}", candidate);
        assert!(
            !candidate.to_string_lossy().contains('\0'),
            "放行了含 NUL 的路径：{:?}",
            candidate
        );
        assert!(
            !candidate
                .components()
                .any(|component| matches!(component, Component::ParentDir)),
            "放行了含 .. 组件的路径：{:?}",
            candidate
        );

        // 归一化后的目标同样必须满足，且不能是顶层根。
        let target = validated.as_path();
        assert!(target.is_absolute(), "归一化结果不是绝对路径：{:?}", target);
        assert!(
            target.components().count() > 2,
            "放行了顶层根目录：{:?}",
            target
        );
    }

    // 生成器必须真的既产生拒绝也产生通过，否则这个测试没有意义。
    assert!(accepted > 100, "生成式测试放行样本过少（{accepted} 条），用例可能失效");
}

#[test]
fn generated_paths_never_accept_control_characters() {
    let ctx = DeletionContext::new();
    let mut rng = XorShift::new(0xdead_beef_0bad_f00d);
    for _ in 0..5_000 {
        let candidate = random_candidate(&mut rng);
        if validate_path_for_deletion(&candidate, &ctx).is_ok() {
            let rendered = candidate.to_string_lossy();
            assert!(
                !rendered.chars().any(|c| c == '\0' || c.is_control()),
                "放行了含控制字符的路径：{:?}",
                candidate
            );
        }
    }
}
