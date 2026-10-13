//! Adversarial tests for the path validator.
//!
//! Two parts:
//! 1. Corpus-driven: every entry in `tests/fuzz_corpus/dangerous_paths.txt` must be
//!    rejected;
//! 2. Generative: build many paths from a deterministic pseudo-random generator and
//!    assert the invariant — **any path that passes validation must be absolute,
//!    contain no NUL, contain no `..` component, and not be a top-level root.**

use std::path::{Component, Path, PathBuf};

use worktree_core::{validate_path_for_deletion, DeletionContext};

const CORPUS: &str = include_str!("fuzz_corpus/dangerous_paths.txt");

/// Lower bound on the number of corpus entries (P0 acceptance criterion: ≥ 60).
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
        "the adversarial corpus needs at least {MIN_CORPUS_ENTRIES} entries, currently {}",
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
        "the following corpus entries should have been rejected but passed validation: {:#?}",
        accepted
    );
}

#[test]
fn corpus_covers_each_required_category() {
    // The corpus must cover the categories claimed by the P0 acceptance requirements,
    // so they cannot be silently emptied later.
    let joined = corpus_entries().join("\n");
    for needle in ["/", "..", "/System", "/etc", "/usr"] {
        assert!(
            joined.contains(needle),
            "the corpus should cover the category: {needle}"
        );
    }
    assert!(
        corpus_entries().iter().any(|entry| entry.contains('\t')),
        "the corpus should include a control-character case"
    );
    assert!(
        corpus_entries()
            .iter()
            .any(|entry| !Path::new(entry).is_absolute()),
        "the corpus should include a relative-path case"
    );
}

/// Deterministic pseudo-random generator (xorshift64*), to avoid pulling in an extra
/// dependency for a single property test.
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
    "..",
    ".",
    "a",
    "bb",
    "etc",
    "System",
    "usr",
    "usr/local",
    "bin",
    "tmp",
    "Users",
    "andy",
    "MySrc",
    "node_modules",
    "target",
    "dev",
    "var",
    "db",
    "db",
    "Library",
    "Extensions",
    "name..files",
    " ",
    "\t",
    "\0",
    "passwd",
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

        // Contract: the raw path that passes validation must be absolute, contain no
        // NUL, and contain no `..` component.
        assert!(
            candidate.is_absolute(),
            "accepted a relative path: {:?}",
            candidate
        );
        assert!(
            !candidate.to_string_lossy().contains('\0'),
            "accepted a path containing NUL: {:?}",
            candidate
        );
        assert!(
            !candidate
                .components()
                .any(|component| matches!(component, Component::ParentDir)),
            "accepted a path containing a .. component: {:?}",
            candidate
        );

        // The normalised target must satisfy the same constraints, and must not be a
        // top-level root.
        let target = validated.as_path();
        assert!(
            target.is_absolute(),
            "normalised result is not an absolute path: {:?}",
            target
        );
        assert!(
            target.components().count() > 2,
            "accepted a top-level root: {:?}",
            target
        );
    }

    // The generator must genuinely produce both rejections and acceptances, otherwise
    // this test is meaningless.
    assert!(
        accepted > 100,
        "the generative test accepted too few samples ({accepted}), the case may be ineffective"
    );
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
                "accepted a path containing control characters: {:?}",
                candidate
            );
        }
    }
}
