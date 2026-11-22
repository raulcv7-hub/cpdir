//! Strict XML serialization for optional `<git_context>` metadata.

use std::fmt::Write as _;

use crate::domain::{
    GitCommitEntry, GitContextSnapshot, GitDiffNumstatEntry, GitDiffStat, GitHotspotEntry,
    GitTagEntry,
};
use crate::output::xml_escape::push_escaped_xml_text;

/// Renders `<git_context>` as a standalone XML fragment for accurate token accounting.
#[must_use]
pub fn render_git_context_xml(git: &GitContextSnapshot) -> String {
    let mut out = String::with_capacity(1_024);
    render_git_context(git, &mut out);
    out
}

pub(crate) fn render_git_context(git: &GitContextSnapshot, out: &mut String) {
    out.push_str("  <git_context>\n");
    out.push_str("    <branch>");
    push_escaped_xml_text(out, &git.branch);
    out.push_str("</branch>\n");

    render_working_tree(&git.working_tree_lines, out);
    render_working_diff_numstat(&git.working_diff_numstat, out);
    render_recent_commits(&git.recent_commits, out);
    render_recent_tags(&git.recent_tags, out);
    render_hotspots(&git.hotspots, out);
    out.push_str("  </git_context>\n");
}

fn render_working_tree(lines: &[String], out: &mut String) {
    if lines.is_empty() {
        out.push_str("    <working_tree/>\n");
        return;
    }
    out.push_str("    <working_tree>\n");
    for line in lines {
        out.push_str("      ");
        push_escaped_xml_text(out, line);
        out.push('\n');
    }
    out.push_str("    </working_tree>\n");
}

fn render_working_diff_numstat(entries: &[GitDiffNumstatEntry], out: &mut String) {
    if entries.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "    <working_diff_numstat count=\"{}\">",
        entries.len()
    );
    for entry in entries {
        match entry.stat {
            GitDiffStat::Text { added, deleted } => {
                let _ = write!(out, "      +{added} -{deleted} ");
            }
            GitDiffStat::Binary => {
                out.push_str("      binary ");
            }
        }
        push_escaped_xml_text(out, &entry.rel_path);
        out.push('\n');
    }
    out.push_str("    </working_diff_numstat>\n");
}

fn render_recent_commits(commits: &[GitCommitEntry], out: &mut String) {
    let _ = writeln!(out, "    <recent_commits count=\"{}\">", commits.len());
    for commit in commits {
        out.push_str("      ");
        push_escaped_xml_text(out, &commit.short_hash);
        out.push_str(" | ");
        push_escaped_xml_text(out, &commit.subject);
        out.push('\n');
        if let Some(body) = &commit.body {
            for body_line in body.lines() {
                out.push_str("        ");
                push_escaped_xml_text(out, body_line);
                out.push('\n');
            }
        }
    }
    out.push_str("    </recent_commits>\n");
}

fn render_recent_tags(tags: &[GitTagEntry], out: &mut String) {
    if tags.is_empty() {
        return;
    }
    let _ = writeln!(out, "    <recent_tags count=\"{}\">", tags.len());
    for tag in tags {
        out.push_str("      ");
        push_escaped_xml_text(out, &tag.name);
        if let Some(summary) = &tag.summary {
            out.push_str(" | ");
            push_escaped_xml_text(out, summary);
        }
        out.push('\n');
    }
    out.push_str("    </recent_tags>\n");
}

fn render_hotspots(hotspots: &[GitHotspotEntry], out: &mut String) {
    if hotspots.is_empty() {
        return;
    }
    out.push_str("    <hotspots>\n");
    for spot in hotspots {
        out.push_str("      ");
        push_escaped_xml_text(out, &spot.rel_path);
        let _ = writeln!(out, " ({} commits)", spot.commits);
    }
    out.push_str("    </hotspots>\n");
}
