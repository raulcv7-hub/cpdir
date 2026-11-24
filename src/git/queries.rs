//! Individual hardened `git` CLI queries scoped to `prefix` inside the repository.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::domain::{GitCommitEntry, GitDiffNumstatEntry, GitHotspotEntry, GitTagEntry};
use crate::git::context::GitContextConfig;
use crate::git::history_parser::{parse_hotspots, parse_recent_commits, parse_recent_tags};
use crate::git::runner::run_git_hardened;
use crate::git::status_parser::{parse_branch_and_status, parse_diff_numstat};

pub(super) fn query_branch_and_status(
    repo_dir: &Path,
    config: &GitContextConfig,
    prefix: &str,
    cancel: Option<&AtomicBool>,
) -> (String, Vec<String>) {
    let mut args = vec!["status", "--porcelain=v1", "-b"];
    if !prefix.is_empty() {
        args.extend(["--", "."]);
    }
    let Some(raw) = run_git_hardened(repo_dir, &args, cancel) else {
        return ("unknown".to_owned(), Vec::new());
    };
    parse_branch_and_status(&raw, prefix, config.max_working_tree_lines)
}

pub(super) fn query_diff_numstat(
    repo_dir: &Path,
    config: &GitContextConfig,
    prefix: &str,
    cancel: Option<&AtomicBool>,
) -> Vec<GitDiffNumstatEntry> {
    if config.max_diff_numstat_lines == 0 {
        return Vec::new();
    }
    let mut head_args = vec!["diff", "HEAD", "--numstat", "--no-ext-diff", "--no-color"];
    if !prefix.is_empty() {
        head_args.extend(["--relative", "--", "."]);
    }
    if let Some(raw) = run_git_hardened(repo_dir, &head_args, cancel) {
        return parse_diff_numstat(&raw, config.max_diff_numstat_lines);
    }

    // Fallback for unborn repositories that have no HEAD commit yet.
    let mut cached_args = vec![
        "diff",
        "--cached",
        "--numstat",
        "--no-ext-diff",
        "--no-color",
    ];
    if !prefix.is_empty() {
        cached_args.extend(["--relative", "--", "."]);
    }
    let Some(raw) = run_git_hardened(repo_dir, &cached_args, cancel) else {
        return Vec::new();
    };
    parse_diff_numstat(&raw, config.max_diff_numstat_lines)
}

pub(super) fn query_recent_commits(
    repo_dir: &Path,
    config: &GitContextConfig,
    prefix: &str,
    cancel: Option<&AtomicBool>,
) -> Vec<GitCommitEntry> {
    if config.max_commits == 0 {
        return Vec::new();
    }
    let limit = format!("-{}", config.max_commits);
    let mut args = vec![
        "log",
        limit.as_str(),
        "--no-show-signature",
        "--format=%h%x1f%s%x1f%b%x00",
    ];
    if !prefix.is_empty() {
        args.extend(["--", "."]);
    }
    let Some(raw) = run_git_hardened(repo_dir, &args, cancel) else {
        return Vec::new();
    };
    parse_recent_commits(&raw, config.max_commit_body_chars)
}

pub(super) fn query_recent_tags(
    repo_dir: &Path,
    config: &GitContextConfig,
    cancel: Option<&AtomicBool>,
) -> Vec<GitTagEntry> {
    if config.max_tags == 0 {
        return Vec::new();
    }
    let count_arg = format!("--count={}", config.max_tags);
    let Some(raw) = run_git_hardened(
        repo_dir,
        &[
            "for-each-ref",
            "--sort=-creatordate",
            &count_arg,
            "--format=%(refname:short)%1f%(contents:subject)%1f%(contents:body)%00",
            "refs/tags",
        ],
        cancel,
    ) else {
        return Vec::new();
    };
    parse_recent_tags(&raw, config.max_tag_summary_chars)
}

pub(super) fn query_hotspots(
    repo_dir: &Path,
    config: &GitContextConfig,
    prefix: &str,
    cancel: Option<&AtomicBool>,
) -> Vec<GitHotspotEntry> {
    if config.max_hotspots == 0 || config.hotspot_commit_window == 0 {
        return Vec::new();
    }
    let limit = format!("-{}", config.hotspot_commit_window);
    let mut args = vec![
        "log",
        limit.as_str(),
        "--no-show-signature",
        "--name-only",
        "--format=",
    ];
    if !prefix.is_empty() {
        // Scope hotspot commit history and emitted file paths strictly to the opened subfolder.
        args.extend(["--relative", "--", "."]);
    }
    let Some(raw) = run_git_hardened(repo_dir, &args, cancel) else {
        return Vec::new();
    };
    parse_hotspots(&raw, config.max_hotspots)
}
