//! Adversarial tests for the path validator.
//!
//! Two parts:
//! 1. Corpus-driven: every entry in `tests/fuzz_corpus/dangerous_paths.txt` must be
//!    rejected;
//! 2. Generative: build many paths from a deterministic pseudo-random generator and
//!    assert the invariant — **any path that passes validation must be absolute,
//!    contain no NUL, contain no `..` component, and not be a top-level root.**

use std::path::{Component, Path, PathBuf};

use worktree_core::{validate_path_for_deletion, DeletionContext, PathRejection};

const CORPUS: &str = include_str!("fuzz_corpus/dangerous_paths.txt");

/// The P0 acceptance criterion is "≥ 60", and this is deliberately higher: the corpus
/// is a file, so without a floor at its current size it could shrink and nothing would
/// complain. Raise this when entries are added; never lower it to make a deletion pass.
const MIN_CORPUS_ENTRIES: usize = 86;

/// The unit test that covers the "ancestor symlink" category. A symlink cannot be
/// expressed as a static corpus line, so the corpus test names the test that covers it
/// and asserts it still exists (see `corpus_covers_each_required_category`).
const ANCESTOR_SYMLINK_TEST: &str = "rejects_symlinked_ancestor_pointing_into_protected_location";

/// The path validator source, so the coverage assertion above is enforced against the
/// real file rather than asserted blindly.
const PATH_SOURCE: &str = include_str!("../src/path.rs");

/// A context whose expected parent never coincides with a corpus entry.
fn ctx() -> DeletionContext {
    DeletionContext::new("/expected/parent")
}

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
    let ctx = ctx();
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
    // P0-3 names five categories: `..`, ancestor symlinks, control characters, system
    // roots, and empty paths. This test enforces each one honestly: the two that cannot
    // be expressed as a static corpus line (an empty path and a symlink) are asserted
    // here directly instead of being claimed by a substring match.
    let entries = corpus_entries();
    let joined = entries.join("\n");

    // 1. `..` as a full path component.
    assert!(
        joined.contains(".."),
        "the corpus must cover `..` traversal"
    );

    // 2. System roots.
    for needle in ["/System", "/etc", "/usr", "/bin"] {
        assert!(
            joined.contains(needle),
            "the corpus must cover the system root {needle}"
        );
    }

    // 3. Control characters.
    assert!(
        entries
            .iter()
            .any(|entry| entry.chars().any(char::is_control)),
        "the corpus must include a control-character case"
    );

    // 4. Relative / non-absolute paths.
    assert!(
        entries.iter().any(|entry| !Path::new(entry).is_absolute()),
        "the corpus must include a relative-path case"
    );

    // 5. Empty path. A line-based corpus cannot represent the empty string, so it is
    // asserted directly; without this the category would only be claimed, never checked.
    assert_eq!(
        validate_path_for_deletion(Path::new(""), &ctx()),
        Err(PathRejection::Empty),
        "the empty path must be rejected"
    );

    // 6. Ancestor symlinks. A symlink cannot be represented as a static string either;
    // the category is covered by a unit test, and this assertion fails if that test is
    // renamed or deleted.
    assert!(
        PATH_SOURCE.contains(&format!("fn {ANCESTOR_SYMLINK_TEST}")),
        "the ancestor-symlink category is covered by the unit test `{ANCESTOR_SYMLINK_TEST}` \
         in src/path.rs; renaming or deleting it must be reflected here"
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
    let ctx = ctx();
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
    let ctx = ctx();
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
