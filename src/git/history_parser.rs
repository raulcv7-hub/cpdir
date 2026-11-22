//! Pure parsers for Git commit logs, tag annotations, and hotspot counts.

use std::collections::HashMap;

use crate::domain::{GitCommitEntry, GitHotspotEntry, GitTagEntry};

/// Parses NUL-delimited `git log` records into truncated [`GitCommitEntry`] items.
#[must_use]
pub(crate) fn parse_recent_commits(raw: &str, max_body_chars: usize) -> Vec<GitCommitEntry> {
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
            .and_then(|raw_body| format_optional_summary(raw_body, max_body_chars));

        commits.push(GitCommitEntry {
            short_hash: short_hash.trim().to_owned(),
            subject,
            body,
        });
    }
    commits
}

/// Parses NUL-delimited `git for-each-ref` tag records into [`GitTagEntry`] items.
#[must_use]
pub(crate) fn parse_recent_tags(raw: &str, max_summary_chars: usize) -> Vec<GitTagEntry> {
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
            (false, true) => format_optional_summary(subject, max_summary_chars),
            (true, false) => format_optional_summary(body, max_summary_chars),
            (false, false) => {
                format_optional_summary(&format!("{subject} - {body}"), max_summary_chars)
            }
        };
        tags.push(GitTagEntry {
            name: name.trim().to_owned(),
            summary: combined,
        });
    }
    tags
}

/// Aggregates `git log --name-only` output into sorted file hotspot counts.
#[must_use]
pub(crate) fn parse_hotspots(raw: &str, max_hotspots: usize) -> Vec<GitHotspotEntry> {
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
    spots.truncate(max_hotspots);
    spots
}

fn format_optional_summary(raw: &str, max_chars: usize) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || max_chars == 0 {
        return None;
    }
    if trimmed.chars().count() <= max_chars {
        return Some(trimmed.to_owned());
    }
    let mut clipped: String = trimmed.chars().take(max_chars).collect();
    clipped.push_str("...");
    Some(clipped)
}
