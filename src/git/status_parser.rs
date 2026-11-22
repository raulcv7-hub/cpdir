//! Pure parsers for Git branch, porcelain working-tree status, and `--numstat` diff counts.

use crate::domain::{GitDiffNumstatEntry, GitDiffStat};

/// Parses `git status --porcelain=v1 -b` output into `(branch, working_tree_lines)`.
#[must_use]
pub(crate) fn parse_branch_and_status(
    raw: &str,
    prefix: &str,
    max_lines: usize,
) -> (String, Vec<String>) {
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
        if working_tree_lines.len() < max_lines {
            working_tree_lines.push(normalize_porcelain_line(line, prefix));
        } else {
            overflow += 1;
        }
    }
    if overflow > 0 {
        working_tree_lines.push(format!("... (+{overflow} more)"));
    }

    (branch, working_tree_lines)
}

/// Parses `git diff --numstat` output capped at `max_lines` entries.
#[must_use]
pub(crate) fn parse_diff_numstat(raw: &str, max_lines: usize) -> Vec<GitDiffNumstatEntry> {
    if max_lines == 0 {
        return Vec::new();
    }
    let mut entries = Vec::new();
    for line in raw.lines() {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let (Some(add_str), Some(del_str), Some(path_str)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let add_trim = add_str.trim();
        let del_trim = del_str.trim();
        let stat = if add_trim == "-" && del_trim == "-" {
            GitDiffStat::Binary
        } else if let (Ok(added), Ok(deleted)) =
            (add_trim.parse::<usize>(), del_trim.parse::<usize>())
        {
            GitDiffStat::Text { added, deleted }
        } else {
            continue;
        };
        entries.push(GitDiffNumstatEntry {
            stat,
            rel_path: path_str.to_owned(),
        });
        if entries.len() >= max_lines {
            break;
        }
    }
    entries
}

fn parse_branch_header(line: &str) -> String {
    let trimmed = line.strip_prefix("## ").unwrap_or(line).trim();
    if trimmed.is_empty() {
        "HEAD".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn normalize_porcelain_line(line: &str, prefix: &str) -> String {
    if prefix.is_empty() {
        return line.to_owned();
    }
    let (Some(status), Some(payload)) = (line.get(..3), line.get(3..)) else {
        return line.to_owned();
    };
    let is_rename_or_copy = matches!(status.as_bytes().first(), Some(b'R' | b'C'));
    if is_rename_or_copy && let Some((old_path, new_path)) = payload.split_once(" -> ") {
        let old_rel = old_path.strip_prefix(prefix).unwrap_or(old_path);
        let new_rel = new_path.strip_prefix(prefix).unwrap_or(new_path);
        return format!("{status}{old_rel} -> {new_rel}");
    }
    let rel = payload.strip_prefix(prefix).unwrap_or(payload);
    format!("{status}{rel}")
}
