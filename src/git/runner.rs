//! Hardened Git subprocess invocation isolated from untrusted repository configuration.

use std::path::Path;
use std::process::Command;

/// Checks whether `repo_dir` belongs to an active Git working tree.
#[must_use]
pub(crate) fn is_inside_work_tree(repo_dir: &Path) -> bool {
    run_git_hardened(repo_dir, &["rev-parse", "--is-inside-work-tree"])
        .is_some_and(|out| out.trim() == "true")
}

/// Executes a read-only `git` command in `repo_dir` with repository hooks neutralized.
#[must_use]
pub(crate) fn run_git_hardened(repo_dir: &Path, args: &[&str]) -> Option<String> {
    let output = hardened_git_command(repo_dir).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
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
