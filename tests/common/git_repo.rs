//! Isolated real Git repository harness for integration and security tests.

use std::fs;
use std::path::Path;
use std::process::Command;

/// Helper wrapper operating on an isolated real Git repository on disk.
pub(crate) struct GitRepoHarness<'a> {
    root: &'a Path,
}

impl<'a> GitRepoHarness<'a> {
    /// Initializes a Git repository at `root` with deterministic local settings.
    pub(crate) fn init(root: &'a Path) -> Self {
        let harness = Self { root };
        harness.run_git(&["init", "-b", "main"]);
        harness.set_local_config("user.name", "cpdir-test");
        harness.set_local_config("user.email", "test@cpdir.local");
        harness.set_local_config("core.autocrlf", "false");
        harness.set_local_config("commit.gpgsign", "false");
        harness
    }

    /// Sets a key in the repository-local `.git/config`.
    pub(crate) fn set_local_config(&self, key: &str, value: &str) {
        self.run_git(&["config", "--local", key, value]);
    }

    /// Stages all working tree changes and creates a commit with optional body text.
    pub(crate) fn commit_all(&self, subject: &str, body: Option<&str>) -> String {
        self.run_git(&["add", "-A"]);
        match body {
            Some(msg_body) => self.run_git(&["commit", "-m", subject, "-m", msg_body]),
            None => self.run_git(&["commit", "-m", subject]),
        };
        self.run_git(&["rev-parse", "--short", "HEAD"])
    }

    /// Writes a custom raw snippet directly into `.git/config` for adversarial hooks tests.
    pub(crate) fn append_raw_git_config(&self, snippet: &str) {
        let config_path = self.root.join(".git").join("config");
        let existing = fs::read_to_string(&config_path).expect("failed to read .git/config");
        let updated = format!("{existing}\n{snippet}\n");
        fs::write(&config_path, updated).expect("failed to write .git/config");
    }

    /// Runs a `git` command in `root` and returns trimmed standard output.
    pub(crate) fn run_git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(self.root)
            .output()
            .expect("failed to spawn git executable; ensure git is installed in PATH");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "git {args:?} failed: {stderr}");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }
}
