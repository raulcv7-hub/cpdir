//! Configuration and high-level snapshot assembly for scoped `<git_context>` extraction.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::config::limits::{
    DEFAULT_GIT_HOTSPOT_COMMIT_WINDOW, DEFAULT_GIT_MAX_COMMIT_BODY_CHARS, DEFAULT_GIT_MAX_COMMITS,
    DEFAULT_GIT_MAX_DIFF_NUMSTAT_LINES, DEFAULT_GIT_MAX_HOTSPOTS,
    DEFAULT_GIT_MAX_TAG_SUMMARY_CHARS, DEFAULT_GIT_MAX_TAGS, DEFAULT_GIT_MAX_WORKING_TREE_LINES,
};
use crate::domain::GitContextSnapshot;
use crate::git::queries::{
    query_branch_and_status, query_diff_numstat, query_hotspots, query_recent_commits,
    query_recent_tags,
};
use crate::git::runner::{GitAvailability, probe_work_tree};

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
            max_commits: DEFAULT_GIT_MAX_COMMITS,
            max_commit_body_chars: DEFAULT_GIT_MAX_COMMIT_BODY_CHARS,
            max_tags: DEFAULT_GIT_MAX_TAGS,
            max_tag_summary_chars: DEFAULT_GIT_MAX_TAG_SUMMARY_CHARS,
            max_hotspots: DEFAULT_GIT_MAX_HOTSPOTS,
            hotspot_commit_window: DEFAULT_GIT_HOTSPOT_COMMIT_WINDOW,
            max_working_tree_lines: DEFAULT_GIT_MAX_WORKING_TREE_LINES,
            max_diff_numstat_lines: DEFAULT_GIT_MAX_DIFF_NUMSTAT_LINES,
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

/// Extracts Git repository metadata for `repo_dir` with cooperative cancellation support.
#[must_use]
pub fn extract_git_context_cancelable(
    repo_dir: &Path,
    config: &GitContextConfig,
    cancel: &AtomicBool,
) -> Option<GitContextSnapshot> {
    extract_git_context_inner(repo_dir, config, Some(cancel)).into_option()
}

/// Extracts Git metadata for `repo_dir` while preserving repository availability diagnostics.
#[must_use]
pub fn extract_git_context_status(
    repo_dir: &Path,
    config: &GitContextConfig,
) -> GitAvailability<GitContextSnapshot> {
    extract_git_context_inner(repo_dir, config, None)
}

fn extract_git_context_inner(
    repo_dir: &Path,
    config: &GitContextConfig,
    cancel: Option<&AtomicBool>,
) -> GitAvailability<GitContextSnapshot> {
    let prefix = match probe_work_tree(repo_dir, cancel) {
        GitAvailability::Available(prefix) => prefix,
        GitAvailability::NotARepository => return GitAvailability::NotARepository,
        GitAvailability::Unavailable { reason } => {
            return GitAvailability::Unavailable { reason };
        }
    };

    let (branch, working_tree_lines) = query_branch_and_status(repo_dir, config, &prefix, cancel);
    if is_cancelled(cancel) {
        return cancelled_status();
    }
    let working_diff_numstat = query_diff_numstat(repo_dir, config, &prefix, cancel);
    if is_cancelled(cancel) {
        return cancelled_status();
    }
    let recent_commits = query_recent_commits(repo_dir, config, &prefix, cancel);
    if is_cancelled(cancel) {
        return cancelled_status();
    }
    let recent_tags = query_recent_tags(repo_dir, config, cancel);
    if is_cancelled(cancel) {
        return cancelled_status();
    }
    let hotspots = query_hotspots(repo_dir, config, &prefix, cancel);

    GitAvailability::Available(GitContextSnapshot {
        branch,
        working_tree_lines,
        working_diff_numstat,
        recent_commits,
        recent_tags,
        hotspots,
    })
}

fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|flag| flag.load(Ordering::Acquire))
}

fn cancelled_status() -> GitAvailability<GitContextSnapshot> {
    GitAvailability::Unavailable {
        reason: "git extraction cancelled".to_owned(),
    }
}
