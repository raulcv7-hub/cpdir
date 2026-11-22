//! Real Git history, tags, hotspots, `--numstat`, unborn repos, and TUI Git-token toggles.

use std::collections::HashMap;

use cpdir::domain::{DirectoryTree, GitDiffNumstatEntry, GitDiffStat};
use cpdir::git::{
    GitAvailability, GitContextConfig, extract_git_context, extract_git_context_status,
};
use cpdir::output::{render_git_context_xml, render_strict_xml};
use cpdir::scanner::{ScanConfig, scan_directory};
use cpdir::tokens::TokenCounter;
use cpdir::tui::TuiAppState;

use crate::common::git_repo::GitRepoHarness;
use crate::common::workspace::{WorkspaceBuilder, sample_png_bytes};
use crate::common::xml_validator::parse_strict_xml;

fn setup_multi_commit_fixture(ws: &WorkspaceBuilder) {
    ws.write_text("src/main.rs", "fn main() {}\n");
    ws.write_text("src/café.rs", "pub fn cafe() {}\n");

    let git = GitRepoHarness::init(ws.root());
    git.commit_all(
        "feat: add <core> & café module",
        Some("line 1\nline 2 with ]]>"),
    );
    git.run_git(&[
        "tag",
        "-a",
        "v0.1.0",
        "-m",
        "Initial <alpha> release\n\nFirst milestone & details",
    ]);

    ws.write_text("src/main.rs", "fn main() { let v = 2; }\n");
    git.commit_all("refactor: step 2", None);

    ws.write_text("src/main.rs", "fn main() { let v = 3; }\n");
    git.commit_all("fix: step 3", None);

    let emoji_body = "🚀".repeat(400);
    ws.write_text("README.md", "# Demo\n");
    git.commit_all("docs: emoji body", Some(&emoji_body));
    git.run_git(&["tag", "-a", "v0.2.0", "-m", "Beta release 🚀"]);

    ws.write_text("src/main.rs", "fn main() { let v = 4; }\n");
    ws.write_text("notes.txt", "pending note\n");
    ws.write_bytes("assets/logo.png", &sample_png_bytes());
    git.run_git(&["add", "assets/logo.png"]);
}

#[test]
fn real_git_repo_extracts_commits_tags_hotspots_and_numstat() {
    let ws = WorkspaceBuilder::new();
    setup_multi_commit_fixture(&ws);

    let ctx = extract_git_context(ws.root(), &GitContextConfig::default())
        .expect("git context must be detected");
    assert!(ctx.branch.contains("main"));
    assert!(
        ctx.working_tree_lines
            .iter()
            .any(|l| l.contains("M src/main.rs"))
    );
    assert!(ctx.working_tree_lines.iter().any(|l| l == "?? notes.txt"));
    assert_eq!(
        ctx.working_diff_numstat,
        vec![
            GitDiffNumstatEntry {
                stat: GitDiffStat::Binary,
                rel_path: "assets/logo.png".to_owned(),
            },
            GitDiffNumstatEntry {
                stat: GitDiffStat::Text {
                    added: 1,
                    deleted: 1,
                },
                rel_path: "src/main.rs".to_owned(),
            },
        ]
    );

    assert_eq!(ctx.recent_commits.len(), 4);
    let truncated_body = ctx.recent_commits[0].body.as_deref().expect("clipped body");
    assert!(truncated_body.ends_with("..."));
    assert_eq!(truncated_body.chars().count(), 303);
    assert_eq!(
        ctx.recent_commits[3].subject,
        "feat: add <core> & café module"
    );
    assert_eq!(
        ctx.recent_commits[3].body.as_deref(),
        Some("line 1\nline 2 with ]]>")
    );

    let tag_names: Vec<&str> = ctx.recent_tags.iter().map(|t| t.name.as_str()).collect();
    assert!(tag_names.contains(&"v0.1.0"));
    assert!(tag_names.contains(&"v0.2.0"));
    let v1_tag = ctx
        .recent_tags
        .iter()
        .find(|t| t.name == "v0.1.0")
        .expect("v0.1.0");
    assert!(
        v1_tag
            .summary
            .as_deref()
            .is_some_and(|s| s.contains("Initial <alpha> release"))
    );

    assert_eq!(ctx.hotspots[0].rel_path, "src/main.rs");
    assert_eq!(ctx.hotspots[0].commits, 3);
    assert!(ctx.hotspots.iter().any(|h| h.rel_path == "src/café.rs"));
}

#[test]
fn git_context_xml_renders_numstat_and_toggles_token_count() {
    let ws = WorkspaceBuilder::new();
    setup_multi_commit_fixture(&ws);

    let ctx = extract_git_context(ws.root(), &GitContextConfig::default())
        .expect("git context must be detected");
    let scan = scan_directory(ws.root(), &ScanConfig::default()).expect("scan");
    let mut ui = TuiAppState::new(scan.tree, true);
    let git_tokens = TokenCounter::o200k()
        .expect("o200k")
        .count(&render_git_context_xml(&ctx));
    ui.set_git_context_tokens(git_tokens);
    assert!(ui.include_git_context());
    assert!(ui.total_effective_tokens() > 0);

    let xml_on = render_strict_xml(ui.tree(), &scan.contents, Some(&ctx)).expect("xml on");
    let parsed_on = parse_strict_xml(&xml_on).expect("valid xml with git");
    assert!(parsed_on.has_git_context);
    assert!(xml_on.contains("<working_diff_numstat count=\"2\">"));
    assert!(xml_on.contains("      binary assets/logo.png"));
    assert!(xml_on.contains("      +1 -1 src/main.rs"));
    assert!(xml_on.contains("<recent_tags count=\"2\">"));
    assert!(xml_on.contains("src/café.rs (1 commits)"));

    ui.toggle_git_context();
    assert!(!ui.include_git_context());
    assert_eq!(ui.total_effective_tokens(), 0);
    let xml_off = render_strict_xml(ui.tree(), &scan.contents, None).expect("xml off");
    let parsed_off = parse_strict_xml(&xml_off).expect("valid xml without git");
    assert!(!parsed_off.has_git_context);
}

#[test]
fn handles_non_git_directories_and_unborn_repositories_gracefully() {
    let non_git = WorkspaceBuilder::new();
    non_git.write_text("hello.rs", "fn hi() {}\n");
    assert_eq!(
        extract_git_context_status(non_git.root(), &GitContextConfig::default()),
        GitAvailability::NotARepository
    );

    let unborn = WorkspaceBuilder::new();
    unborn.write_text("draft.txt", "uncommitted\n");
    unborn.write_text("staged.rs", "line 1\nline 2\n");
    let git = GitRepoHarness::init(unborn.root());
    git.run_git(&["add", "staged.rs"]);

    let ctx = extract_git_context(unborn.root(), &GitContextConfig::default())
        .expect("unborn repo still returns context");
    assert!(ctx.branch.contains("main"));
    assert!(ctx.working_tree_lines.iter().any(|l| l == "?? draft.txt"));
    assert_eq!(
        ctx.working_diff_numstat,
        vec![GitDiffNumstatEntry {
            stat: GitDiffStat::Text {
                added: 2,
                deleted: 0,
            },
            rel_path: "staged.rs".to_owned(),
        }]
    );
    assert_eq!(ctx.recent_commits, vec![]);
    assert_eq!(ctx.recent_tags, vec![]);
    assert_eq!(ctx.hotspots, vec![]);

    let empty_contents: HashMap<_, String> = HashMap::new();
    let xml = render_strict_xml(&DirectoryTree::new("unborn"), &empty_contents, Some(&ctx))
        .expect("xml renders unborn repo");
    assert!(parse_strict_xml(&xml).expect("valid xml").has_git_context);
}
