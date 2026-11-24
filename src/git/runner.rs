//! Hardened Git subprocess invocation isolated from untrusted config, LFS traps, and hangs.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::config::limits::GIT_COMMAND_TIMEOUT_MS;

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
pub(crate) fn probe_work_tree(
    repo_dir: &Path,
    cancel: Option<&AtomicBool>,
) -> GitAvailability<String> {
    let out = match run_git_raw_with_control(
        repo_dir,
        &["rev-parse", "--is-inside-work-tree", "--show-prefix"],
        Duration::from_millis(GIT_COMMAND_TIMEOUT_MS),
        cancel,
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

/// Executes a bounded read-only `git` command in `repo_dir` with repository hooks neutralized.
#[must_use]
pub(crate) fn run_git_hardened(
    repo_dir: &Path,
    args: &[&str],
    cancel: Option<&AtomicBool>,
) -> Option<String> {
    let out = run_git_raw_with_control(
        repo_dir,
        args,
        Duration::from_millis(GIT_COMMAND_TIMEOUT_MS),
        cancel,
    )
    .ok()?;
    out.success.then_some(out.stdout)
}

fn run_git_raw_with_control(
    repo_dir: &Path,
    args: &[&str],
    timeout: Duration,
    cancel: Option<&AtomicBool>,
) -> Result<GitRawOutput, String> {
    if cancel.is_some_and(|flag| flag.load(Ordering::Acquire)) {
        return Err("git query cancelled".to_owned());
    }

    let mut child = hardened_git_command(repo_dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| err.to_string())?;

    let stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| "missing git stdout pipe".to_owned())?;
    let stderr_pipe = child
        .stderr
        .take()
        .ok_or_else(|| "missing git stderr pipe".to_owned())?;

    let deadline = Instant::now() + timeout;
    std::thread::scope(|scope| {
        // Drain stdout and stderr concurrently so large diffs cannot fill OS pipe buffers.
        let out_handle = scope.spawn(move || read_pipe_to_end(stdout_pipe));
        let err_handle = scope.spawn(move || read_pipe_to_end(stderr_pipe));

        let status = wait_child_with_control(&mut child, deadline, cancel)?;
        let stdout_bytes = out_handle.join().unwrap_or_default();
        let stderr_bytes = err_handle.join().unwrap_or_default();

        Ok(GitRawOutput {
            success: status.success(),
            stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
            stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
        })
    })
}

fn wait_child_with_control(
    child: &mut Child,
    deadline: Instant,
    cancel: Option<&AtomicBool>,
) -> Result<ExitStatus, String> {
    loop {
        if cancel.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("git query cancelled".to_owned());
        }
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("git query timed out".to_owned());
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(err.to_string());
            }
        }
    }
}

fn read_pipe_to_end<R: Read>(mut reader: R) -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = reader.read_to_end(&mut buf);
    buf
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
    // Neutralize repository-local config and LFS smudge/clean traps before querying untrusted folders.
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
        "-c",
        "filter.lfs.smudge=",
        "-c",
        "filter.lfs.clean=",
        "-c",
        "filter.lfs.process=",
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
