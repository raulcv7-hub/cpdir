//! Hardened Git subprocess invocation isolated from untrusted repository configuration.

use std::path::Path;
use std::process::Command;

/// Availability status returned when probing or extracting Git metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitAvailability<T> {
    /// Target belongs to an accessible Git working tree with extracted payload.
    Available(T),
    /// Target directory is outside any Git working tree.
    NotARepository,
    /// Git binary is unavailable or rejected access (for example `safe.directory`).
    Unavailable {
        /// Diagnostic failure description reported by Git or process spawning.
        reason: String,
    },
}

impl<T> GitAvailability<T> {
    /// Discards diagnostics and returns the inner payload when available.
    #[must_use]
    pub fn into_option(self) -> Option<T> {
        match self {
            Self::Available(val) => Some(val),
            Self::NotARepository | Self::Unavailable { .. } => None,
        }
    }
}

struct GitRawOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

/// Probes whether `repo_dir` is inside a Git working tree and returns its repo-relative prefix.
#[must_use]
pub(crate) fn probe_work_tree(repo_dir: &Path) -> GitAvailability<String> {
    let out = match run_git_raw(
        repo_dir,
        &["rev-parse", "--is-inside-work-tree", "--show-prefix"],
    ) {
        Ok(out) => out,
        Err(reason) => return GitAvailability::Unavailable { reason },
    };

    if !out.success {
        return classify_rev_parse_failure(&out.stderr);
    }

    let mut lines = out.stdout.lines();
    if lines.next().map(str::trim) != Some("true") {
        return GitAvailability::NotARepository;
    }
    let prefix = lines
        .next()
        .unwrap_or_default()
        .trim_end_matches(['\r', '\n']);
    GitAvailability::Available(prefix.to_owned())
}

/// Executes a read-only `git` command in `repo_dir` with repository hooks neutralized.
#[must_use]
pub(crate) fn run_git_hardened(repo_dir: &Path, args: &[&str]) -> Option<String> {
    let out = run_git_raw(repo_dir, args).ok()?;
    out.success.then_some(out.stdout)
}

fn run_git_raw(repo_dir: &Path, args: &[&str]) -> Result<GitRawOutput, String> {
    let output = hardened_git_command(repo_dir)
        .args(args)
        .output()
        .map_err(|err| err.to_string())?;

    Ok(GitRawOutput {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn classify_rev_parse_failure(stderr: &str) -> GitAvailability<String> {
    if stderr.to_ascii_lowercase().contains("not a git repository") {
        return GitAvailability::NotARepository;
    }
    let trimmed = stderr.trim();
    let reason = if trimmed.is_empty() {
        "git rev-parse exited with failure".to_owned()
    } else {
        trimmed.to_owned()
    };
    GitAvailability::Unavailable { reason }
}

fn hardened_git_command(repo_dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    // Neutralize repository-local config traps before querying untrusted folders.
    cmd.args([
        "-c",
        "core.fsmonitor=",
        "-c",
        "core.hooksPath=",
        "-c",
        "core.quotePath=false",
        "-c",
        "diff.external=",
        "-c",
        "gpg.program=",
    ])
    .current_dir(repo_dir)
    .env("GIT_TERMINAL_PROMPT", "0")
    .env("GCM_INTERACTIVE", "never");
    cmd
}

#[cfg(test)]
mod tests {
    use super::{GitAvailability, classify_rev_parse_failure};

    #[test]
    fn classifies_safe_directory_separately_from_non_git_dirs() {
        assert_eq!(
            classify_rev_parse_failure("fatal: not a git repository (or any of the parent dirs)"),
            GitAvailability::NotARepository
        );
        assert_eq!(
            classify_rev_parse_failure(
                "fatal: detected dubious ownership in repository at 'C:/untrusted'"
            ),
            GitAvailability::Unavailable {
                reason: "fatal: detected dubious ownership in repository at 'C:/untrusted'"
                    .to_owned(),
            }
        );
    }
}
