//! Scoped local `git` CLI queries for branch, working tree, diff numstat, commits, tags, and hotspots.

use std::path::Path;

use crate::git::parser::{
    parse_branch_and_status, parse_diff_numstat, parse_hotspots, parse_recent_commits,
    parse_recent_tags,
};
use crate::git::runner::{GitAvailability, probe_work_tree, run_git_hardened};
use crate::output::{
    GitCommitEntry, GitContextSnapshot, GitDiffNumstatEntry, GitHotspotEntry, GitTagEntry,
};

const DEFAULT_MAX_COMMITS: usize = 15;
const DEFAULT_MAX_COMMIT_BODY_CHARS: usize = 300;
const DEFAULT_MAX_TAGS: usize = 5;
const DEFAULT_MAX_TAG_SUMMARY_CHARS: usize = 220;
const DEFAULT_MAX_HOTSPOTS: usize = 10;
const DEFAULT_HOTSPOT_COMMIT_WINDOW: usize = 100;
const DEFAULT_MAX_WORKING_TREE_LINES: usize = 100;
const DEFAULT_MAX_DIFF_NUMSTAT_LINES: usize = 50;

/// Limits controlling `<git_context>` extraction size and history depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitContextConfig {
    /// Maximum recent commits to include.
    pub max_commits: usize,
    /// Character ceiling for each commit body before truncation.
    pub max_commit_body_chars: usize,
    /// Maximum recent tags to include.
    pub max_tags: usize,
    /// Character ceiling for each tag annotation summary.
    pub max_tag_summary_chars: usize,
    /// Maximum hotspot file entries to include.
    pub max_hotspots: usize,
    /// Number of recent commits scanned to compute hotspots.
    pub hotspot_commit_window: usize,
    /// Maximum working-tree status lines before truncating.
    pub max_working_tree_lines: usize,
    /// Maximum modified-file entries emitted by `git diff --numstat`.
    pub max_diff_numstat_lines: usize,
}

impl Default for GitContextConfig {
    fn default() -> Self {
        Self {
            max_commits: DEFAULT_MAX_COMMITS,
            max_commit_body_chars: DEFAULT_MAX_COMMIT_BODY_CHARS,
            max_tags: DEFAULT_MAX_TAGS,
            max_tag_summary_chars: DEFAULT_MAX_TAG_SUMMARY_CHARS,
            max_hotspots: DEFAULT_MAX_HOTSPOTS,
            hotspot_commit_window: DEFAULT_HOTSPOT_COMMIT_WINDOW,
            max_working_tree_lines: DEFAULT_MAX_WORKING_TREE_LINES,
            max_diff_numstat_lines: DEFAULT_MAX_DIFF_NUMSTAT_LINES,
        }
    }
}

/// Extracts Git repository metadata for `repo_dir`, returning `None` outside Git repos.
#[must_use]
pub fn extract_git_context(
    repo_dir: &Path,
    config: &GitContextConfig,
) -> Option<GitContextSnapshot> {
    extract_git_context_status(repo_dir, config).into_option()
}

/// Extracts Git metadata for `repo_dir` while preserving repository availability diagnostics.
#[must_use]
pub fn extract_git_context_status(
    repo_dir: &Path,
    config: &GitContextConfig,
) -> GitAvailability<GitContextSnapshot> {
    let prefix = match probe_work_tree(repo_dir) {
        GitAvailability::Available(prefix) => prefix,
        GitAvailability::NotARepository => return GitAvailability::NotARepository,
        GitAvailability::Unavailable { reason } => {
            return GitAvailability::Unavailable { reason };
        }
    };

    let (branch, working_tree_lines) = query_branch_and_status(repo_dir, config, &prefix);
    let working_diff_numstat = query_diff_numstat(repo_dir, config, &prefix);
    let recent_commits = query_recent_commits(repo_dir, config, &prefix);
    let recent_tags = query_recent_tags(repo_dir, config);
    let hotspots = query_hotspots(repo_dir, config, &prefix);

    GitAvailability::Available(GitContextSnapshot {
        branch,
        working_tree_lines,
        working_diff_numstat,
        recent_commits,
        recent_tags,
        hotspots,
    })
}

fn query_branch_and_status(
    repo_dir: &Path,
    config: &GitContextConfig,
    prefix: &str,
) -> (String, Vec<String>) {
    let mut args = vec!["status", "--porcelain=v1", "-b"];
    if !prefix.is_empty() {
        args.extend(["--", "."]);
    }
    let Some(raw) = run_git_hardened(repo_dir, &args) else {
        return ("unknown".to_owned(), Vec::new());
    };
    parse_branch_and_status(&raw, prefix, config.max_working_tree_lines)
}

fn query_diff_numstat(
    repo_dir: &Path,
    config: &GitContextConfig,
    prefix: &str,
) -> Vec<GitDiffNumstatEntry> {
    if config.max_diff_numstat_lines == 0 {
        return Vec::new();
    }
    let mut head_args = vec!["diff", "HEAD", "--numstat", "--no-ext-diff", "--no-color"];
    if !prefix.is_empty() {
        head_args.extend(["--relative", "--", "."]);
    }
    if let Some(raw) = run_git_hardened(repo_dir, &head_args) {
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
    let Some(raw) = run_git_hardened(repo_dir, &cached_args) else {
        return Vec::new();
    };
    parse_diff_numstat(&raw, config.max_diff_numstat_lines)
}

fn query_recent_commits(
    repo_dir: &Path,
    config: &GitContextConfig,
    prefix: &str,
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
    let Some(raw) = run_git_hardened(repo_dir, &args) else {
        return Vec::new();
    };
    parse_recent_commits(&raw, config.max_commit_body_chars)
}

fn query_recent_tags(repo_dir: &Path, config: &GitContextConfig) -> Vec<GitTagEntry> {
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
    ) else {
        return Vec::new();
    };
    parse_recent_tags(&raw, config.max_tag_summary_chars)
}

fn query_hotspots(
    repo_dir: &Path,
    config: &GitContextConfig,
    prefix: &str,
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
    let Some(raw) = run_git_hardened(repo_dir, &args) else {
        return Vec::new();
    };
    parse_hotspots(&raw, config.max_hotspots)
}
