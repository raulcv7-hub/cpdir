//! Hardened local `git` CLI queries for branch, working tree, commits, tags, and hotspots.

use std::collections::HashMap;
use std::path::Path;

use crate::git::runner::{is_inside_work_tree, run_git_hardened};
use crate::output::{GitCommitEntry, GitContextSnapshot, GitHotspotEntry, GitTagEntry};

const DEFAULT_MAX_COMMITS: usize = 15;
const DEFAULT_MAX_COMMIT_BODY_CHARS: usize = 300;
const DEFAULT_MAX_TAGS: usize = 5;
const DEFAULT_MAX_TAG_SUMMARY_CHARS: usize = 220;
const DEFAULT_MAX_HOTSPOTS: usize = 10;
const DEFAULT_HOTSPOT_COMMIT_WINDOW: usize = 100;
const DEFAULT_MAX_WORKING_TREE_LINES: usize = 100;

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
        }
    }
}

/// Extracts Git repository metadata for `repo_dir`, returning `None` outside Git repos.
#[must_use]
pub fn extract_git_context(
    repo_dir: &Path,
    config: &GitContextConfig,
) -> Option<GitContextSnapshot> {
    if !is_inside_work_tree(repo_dir) {
        return None;
    }

    let (branch, working_tree_lines) = query_branch_and_status(repo_dir, config);
    let recent_commits = query_recent_commits(repo_dir, config);
    let recent_tags = query_recent_tags(repo_dir, config);
    let hotspots = query_hotspots(repo_dir, config);

    Some(GitContextSnapshot {
        branch,
        working_tree_lines,
        recent_commits,
        recent_tags,
        hotspots,
    })
}

fn query_branch_and_status(repo_dir: &Path, config: &GitContextConfig) -> (String, Vec<String>) {
    let Some(raw) = run_git_hardened(repo_dir, &["status", "--porcelain=v1", "-b"]) else {
        return ("unknown".to_owned(), Vec::new());
    };

    let mut lines = raw.lines();
    let branch = lines
        .next()
        .map_or_else(|| "unknown".to_owned(), parse_branch_header);

    let mut working_tree_lines = Vec::new();
    let mut overflow = 0usize;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if working_tree_lines.len() < config.max_working_tree_lines {
            working_tree_lines.push(line.to_owned());
        } else {
            overflow += 1;
        }
    }
    if overflow > 0 {
        working_tree_lines.push(format!("... (+{overflow} more)"));
    }

    (branch, working_tree_lines)
}

fn parse_branch_header(line: &str) -> String {
    let trimmed = line.strip_prefix("## ").unwrap_or(line).trim();
    if trimmed.is_empty() {
        "HEAD".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn query_recent_commits(repo_dir: &Path, config: &GitContextConfig) -> Vec<GitCommitEntry> {
    if config.max_commits == 0 {
        return Vec::new();
    }
    let limit = format!("-{}", config.max_commits);
    let Some(raw) = run_git_hardened(
        repo_dir,
        &[
            "log",
            &limit,
            "--no-show-signature",
            "--format=%h%x1f%s%x1f%b%x00",
        ],
    ) else {
        return Vec::new();
    };

    let mut commits = Vec::new();
    for record in raw.split('\0') {
        let trimmed_rec = record.trim_matches(['\r', '\n']);
        if trimmed_rec.is_empty() {
            continue;
        }
        let mut parts = trimmed_rec.splitn(3, '\x1f');
        let Some(short_hash) = parts.next() else {
            continue;
        };
        let subject = parts.next().unwrap_or_default().trim().to_owned();
        let body = parts
            .next()
            .and_then(|raw_body| format_optional_summary(raw_body, config.max_commit_body_chars));

        commits.push(GitCommitEntry {
            short_hash: short_hash.trim().to_owned(),
            subject,
            body,
        });
    }
    commits
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

    let mut tags = Vec::new();
    for record in raw.split('\0') {
        let trimmed_rec = record.trim_matches(['\r', '\n']);
        if trimmed_rec.is_empty() {
            continue;
        }
        let mut parts = trimmed_rec.splitn(3, '\x1f');
        let Some(name) = parts.next() else {
            continue;
        };
        let subject = parts.next().unwrap_or_default().trim();
        let body = parts.next().unwrap_or_default().trim();
        let combined = match (subject.is_empty(), body.is_empty()) {
            (true, true) => None,
            (false, true) => format_optional_summary(subject, config.max_tag_summary_chars),
            (true, false) => format_optional_summary(body, config.max_tag_summary_chars),
            (false, false) => format_optional_summary(
                &format!("{subject} - {body}"),
                config.max_tag_summary_chars,
            ),
        };
        tags.push(GitTagEntry {
            name: name.trim().to_owned(),
            summary: combined,
        });
    }
    tags
}

fn query_hotspots(repo_dir: &Path, config: &GitContextConfig) -> Vec<GitHotspotEntry> {
    if config.max_hotspots == 0 || config.hotspot_commit_window == 0 {
        return Vec::new();
    }
    let limit = format!("-{}", config.hotspot_commit_window);
    let Some(raw) = run_git_hardened(
        repo_dir,
        &[
            "log",
            &limit,
            "--no-show-signature",
            "--name-only",
            "--format=",
        ],
    ) else {
        return Vec::new();
    };

    let mut counts: HashMap<String, usize> = HashMap::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            *counts.entry(trimmed.to_owned()).or_insert(0) += 1;
        }
    }

    let mut spots: Vec<GitHotspotEntry> = counts
        .into_iter()
        .map(|(rel_path, commits)| GitHotspotEntry { rel_path, commits })
        .collect();
    spots.sort_by(|a, b| {
        b.commits
            .cmp(&a.commits)
            .then_with(|| a.rel_path.cmp(&b.rel_path))
    });
    spots.truncate(config.max_hotspots);
    spots
}

fn format_optional_summary(raw: &str, max_chars: usize) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || max_chars == 0 {
        return None;
    }
    let total_chars = trimmed.chars().count();
    if total_chars <= max_chars {
        return Some(trimmed.to_owned());
    }
    let mut clipped: String = trimmed.chars().take(max_chars).collect();
    clipped.push_str("...");
    Some(clipped)
}
