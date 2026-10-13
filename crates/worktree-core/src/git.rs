//! Read-only Git command wrapper.
//!
//! Every Git call in the scanner goes through [`GitRunner`] (product plan section 5.1:
//! "all data sources are read-only Git commands; the `.git` file format is never
//! parsed"). The runner is a **trait** so tests can inject timeouts, permission
//! errors, and canned output without a real repository (development contract D3).
//!
//! Read-only guarantees enforced by [`ProcessGitRunner`]:
//!
//! - `GIT_OPTIONAL_LOCKS=0` — `git status` refreshes the index and takes
//!   `.git/index.lock` as a side effect; disabling optional locks keeps a scan from
//!   writing to the repository (D1).
//! - `GIT_TERMINAL_PROMPT=0` — a credential prompt would otherwise hang until the
//!   timeout; git must fail instead of waiting for input.
//! - stdin is `/dev/null`, so no subcommand can wait on input.
//!
//! Scanning code must only pass read-only verbs (`status`, `worktree list`,
//! `merge-base`, `stash list`, `ls-files`, `log`, `rev-parse`, `symbolic-ref`). The
//! transport itself does not enforce that: the executor stage owns write commands and
//! will own its own guard.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Default per-command budget. A single Git call on a local repository is fast; ten
/// seconds is generous and keeps a scan from hanging on a stuck process.
pub const DEFAULT_GIT_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll interval while waiting for a child process.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Output of a finished Git command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
    /// Exit status code; `None` when the process was terminated by a signal.
    pub code: Option<i32>,
}

impl GitOutput {
    /// Whether the command succeeded (exit code 0).
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// Every failure mode is distinguishable so the caller can fail closed: a timeout, an
/// unstartable process, and a non-zero exit must never look like an empty-but-fine
/// result.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git command timed out after {seconds}s: {command}")]
    Timeout { command: String, seconds: u64 },
    #[error("git command could not run: {command} ({source})")]
    Io {
        command: String,
        #[source]
        source: std::io::Error,
    },
    #[error("git exited with {code:?}: {command}: {}", stderr.trim())]
    Failed {
        command: String,
        code: Option<i32>,
        stderr: String,
    },
}

/// The read-only Git entry point.
pub trait GitRunner {
    /// Runs `git -C <repo> <args...>` and returns its captured output.
    fn run(&self, repo: &Path, args: &[&str]) -> Result<GitOutput, GitError>;
}

/// Runs real Git processes with a per-command timeout.
#[derive(Debug, Clone)]
pub struct ProcessGitRunner {
    timeout: Duration,
}

impl ProcessGitRunner {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl Default for ProcessGitRunner {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_GIT_TIMEOUT,
        }
    }
}

impl GitRunner for ProcessGitRunner {
    fn run(&self, repo: &Path, args: &[&str]) -> Result<GitOutput, GitError> {
        let command = display_command(repo, args);
        let mut child = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| GitError::Io {
                command: command.clone(),
                source,
            })?;

        let stdout_pipe = child.stdout.take().ok_or_else(|| GitError::Io {
            command: command.clone(),
            source: std::io::Error::other("stdout was not piped"),
        })?;
        let stderr_pipe = child.stderr.take().ok_or_else(|| GitError::Io {
            command: command.clone(),
            source: std::io::Error::other("stderr was not piped"),
        })?;

        // Reader threads drain the pipes while the poll loop waits: a command that
        // writes more than the pipe buffer (64 KiB on macOS) would otherwise block
        // forever and be misreported as a timeout.
        let stdout_thread = std::thread::spawn(move || read_all(stdout_pipe));
        let stderr_thread = std::thread::spawn(move || read_all(stderr_pipe));

        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        // Kill, reap, and let the reader threads hit EOF before
                        // returning: a detached reader could otherwise outlive the
                        // borrowed pipes.
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = stdout_thread.join();
                        let _ = stderr_thread.join();
                        return Err(GitError::Timeout {
                            command,
                            seconds: self.timeout.as_secs(),
                        });
                    }
                    std::thread::sleep(POLL_INTERVAL);
                }
                Err(source) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_thread.join();
                    let _ = stderr_thread.join();
                    return Err(GitError::Io { command, source });
                }
            }
        };

        let stdout = stdout_thread.join().unwrap_or_default();
        let stderr = stderr_thread.join().unwrap_or_default();
        Ok(GitOutput {
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            code: status.code(),
        })
    }
}

fn read_all(mut pipe: impl Read) -> Vec<u8> {
    let mut buffer = Vec::new();
    // A read error on a dead child is not actionable here; the exit status decides.
    let _ = pipe.read_to_end(&mut buffer);
    buffer
}

/// Renders a call as `git -C <repo> <args...>` for error evidence.
pub(crate) fn display_command(repo: &Path, args: &[&str]) -> String {
    let mut text = format!("git -C {}", repo.display());
    for arg in args {
        text.push(' ');
        text.push_str(arg);
    }
    text
}

/// Test doubles for the Git transport.
///
/// They live in their own module so the worktree check tests can inject timeouts,
/// permission errors, and canned output without a real repository (D3).
#[cfg(test)]
pub(crate) mod testing {
    use super::{GitError, GitOutput, GitRunner};
    use std::path::Path;

    /// A canned answer for one exact argument list.
    #[derive(Debug, Clone)]
    pub(crate) enum Canned {
        Out(GitOutput),
        Failed { code: Option<i32>, stderr: String },
        Timeout,
    }

    /// Returns canned output keyed by the argument list.
    ///
    /// The first matching rule wins; an unmatched call fails loudly rather than
    /// silently returning empty output, so a test can never pass because a call was
    /// never made.
    #[derive(Debug, Default)]
    pub(crate) struct FakeGitRunner {
        rules: Vec<(Vec<String>, Canned)>,
        calls: std::sync::Mutex<Vec<Vec<String>>>,
    }

    impl FakeGitRunner {
        pub(crate) fn new(rules: Vec<(Vec<String>, Canned)>) -> Self {
            Self {
                rules,
                calls: std::sync::Mutex::new(Vec::new()),
            }
        }

        pub(crate) fn calls(&self) -> Vec<Vec<String>> {
            self.calls.lock().expect("lock calls").clone()
        }

        /// Whether a call with exactly these arguments was made.
        pub(crate) fn called(&self, args: &[&str]) -> bool {
            let wanted: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
            self.calls().iter().any(|call| call == &wanted)
        }
    }

    impl GitRunner for FakeGitRunner {
        fn run(&self, _repo: &Path, args: &[&str]) -> Result<GitOutput, GitError> {
            let key: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
            self.calls.lock().expect("lock calls").push(key.clone());
            for (matcher, canned) in &self.rules {
                if matcher == &key {
                    return match canned {
                        Canned::Out(output) => Ok(output.clone()),
                        Canned::Failed { code, stderr } => Err(GitError::Failed {
                            command: key.join(" "),
                            code: *code,
                            stderr: stderr.clone(),
                        }),
                        Canned::Timeout => Err(GitError::Timeout {
                            command: key.join(" "),
                            seconds: 10,
                        }),
                    };
                }
            }
            Err(GitError::Failed {
                command: key.join(" "),
                code: Some(128),
                stderr: "no canned response for this call".to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{Canned, FakeGitRunner};
    use super::*;

    fn output(code: Option<i32>, stdout: &str, stderr: &str) -> Canned {
        Canned::Out(GitOutput {
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            code,
        })
    }

    #[test]
    fn fake_runner_reports_every_call() {
        let runner = FakeGitRunner::new(vec![(
            vec!["status".to_string()],
            output(Some(0), "ok", ""),
        )]);
        let result = runner.run(Path::new("/repo"), &["status"]);
        assert!(result.expect("status").success());
        assert!(runner.called(&["status"]));
        assert!(!runner.called(&["stash", "list"]));
    }

    #[test]
    fn fake_runner_fails_loudly_on_an_uncanned_call() {
        let runner = FakeGitRunner::new(vec![]);
        let error = runner
            .run(Path::new("/repo"), &["status"])
            .expect_err("uncanned call must fail");
        assert!(error.to_string().contains("no canned response"));
    }

    #[test]
    fn real_runner_reads_a_real_repository() {
        // Skipped when git is unavailable: the fake runner covers the logic, this
        // test only proves the transport works against a real process.
        let dir = std::env::temp_dir().join(format!(
            "worktree-cleaner-git-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let run = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .output()
                .expect("run git")
        };
        if !run(&["init", "--quiet"]).status.success() {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "Test"]);
        std::fs::write(dir.join("file.txt"), "hello\n").expect("write file");
        run(&["add", "file.txt"]);
        run(&["commit", "--quiet", "-m", "init"]);

        let runner = ProcessGitRunner::default();
        let worktrees = runner
            .run(&dir, &["worktree", "list", "--porcelain"])
            .expect("worktree list");
        assert!(worktrees.success());
        assert!(worktrees.stdout.contains("worktree "));

        let status = runner
            .run(&dir, &["status", "--porcelain=v2", "--branch"])
            .expect("status");
        assert!(status.success());
        assert!(status.stdout.contains("# branch.oid"));

        // A missing directory makes git itself exit non-zero, which is a `Failed`
        // output rather than a transport error: the caller must treat it as a failed
        // check, not as empty-and-fine.
        let missing = dir.join("does-not-exist");
        let output = runner
            .run(&missing, &["status"])
            .expect("git itself answers, with a non-zero exit");
        assert!(!output.success());
        assert!(output.stderr.contains("cannot change to"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn timeout_is_enforced() {
        // A one-nanosecond budget expires before any real command can finish, which
        // is the same path a hung git process takes.
        let runner = ProcessGitRunner::new(Duration::from_nanos(1));
        let error = runner
            .run(Path::new("."), &["--version"])
            .expect_err("instant timeout must fire");
        match error {
            GitError::Timeout { .. } => {}
            other => panic!("expected a timeout, got {other:?}"),
        }
    }
}
