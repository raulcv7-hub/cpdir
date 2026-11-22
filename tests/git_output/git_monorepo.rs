//! Monorepo subdirectory scoping, detached `HEAD`, and configured extraction-cap tests.

use std::fs;

use cpdir::domain::GitDiffStat;
use cpdir::git::{
    GitAvailability, GitContextConfig, extract_git_context, extract_git_context_status,
};

use crate::common::git_repo::GitRepoHarness;
use crate::common::workspace::WorkspaceBuilder;

#[test]
fn monorepo_subdirectory_scopes_status_numstat_commits_hotspots_and_detached_head() {
    let ws = WorkspaceBuilder::new();
    ws.write_text("packages/web/app.ts", "export const web = 1;\n");
    ws.write_text("packages/api/main.rs", "fn main() {}\n");
    ws.write_text(
        "packages/api/old_name.rs",
        "pub fn a() {}\npub fn b() {}\npub fn c() {}\npub fn d() {}\n",
    );
    ws.write_text("packages/api/café guide.rs", "pub fn guide() {}\n");

    let git = GitRepoHarness::init(ws.root());
    git.commit_all("chore: initial monorepo", None);

    ws.write_text("packages/web/app.ts", "export const web = 2;\n");
    git.commit_all("feat(web): web-only commit", None);

    ws.write_text("packages/api/main.rs", "fn main() { println!(\"v2\"); }\n");
    git.commit_all("feat(api): api update", None);

    let api_dir = ws.root().join("packages/api");
    fs::rename(api_dir.join("old_name.rs"), api_dir.join("new_name.rs")).expect("rename");
    git.run_git(&["add", "-A"]);
    ws.write_text("packages/web/app.ts", "export const web = 99;\n");
    ws.write_text("packages/web/dirty_web.ts", "export const dirty = true;\n");
    ws.write_text(
        "packages/api/main.rs",
        "fn main() {\n    println!(\"v3\");\n}\n",
    );
    ws.write_text("packages/api/dirty_api.txt", "pending api note\n");

    let GitAvailability::Available(ctx) =
        extract_git_context_status(&api_dir, &GitContextConfig::default())
    else {
        panic!("expected git context for monorepo subdirectory");
    };

    assert!(
        ctx.working_tree_lines
            .iter()
            .any(|l| l == "R  old_name.rs -> new_name.rs"),
        "expected relative rename line, got {:?}",
        ctx.working_tree_lines
    );
    assert!(
        ctx.working_tree_lines
            .iter()
            .any(|l| l == "?? dirty_api.txt")
    );
    assert!(
        ctx.working_tree_lines
            .iter()
            .all(|l| !l.contains("packages/web") && !l.contains("packages/api/"))
    );

    assert!(
        ctx.working_diff_numstat
            .iter()
            .any(|e| e.rel_path == "main.rs"
                && e.stat
                    == GitDiffStat::Text {
                        added: 3,
                        deleted: 1
                    }),
        "expected relative main.rs numstat, got {:?}",
        ctx.working_diff_numstat
    );
    assert!(
        ctx.working_diff_numstat
            .iter()
            .all(|e| !e.rel_path.contains("web") && !e.rel_path.contains("packages/api/"))
    );

    let subjects: Vec<&str> = ctx
        .recent_commits
        .iter()
        .map(|c| c.subject.as_str())
        .collect();
    assert_eq!(
        subjects,
        vec!["feat(api): api update", "chore: initial monorepo"]
    );

    let hotspot_paths: Vec<&str> = ctx.hotspots.iter().map(|h| h.rel_path.as_str()).collect();
    assert!(hotspot_paths.contains(&"main.rs"));
    assert!(hotspot_paths.contains(&"café guide.rs"));
    assert!(
        hotspot_paths
            .iter()
            .all(|p| !p.contains("packages/web") && !p.starts_with("packages/api/"))
    );

    git.run_git(&["checkout", "--detach", "HEAD"]);
    let detached = extract_git_context(&api_dir, &GitContextConfig::default())
        .expect("detached head still extracts context");
    assert!(detached.branch.contains("HEAD"));
}

#[test]
fn enforces_configured_caps_for_commits_tags_hotspots_numstat_and_working_tree() {
    let ws = WorkspaceBuilder::new();
    let git = GitRepoHarness::init(ws.root());

    for idx in 0..4 {
        ws.write_text(&format!("file_{idx}.rs"), &format!("// {idx}\n"));
        git.commit_all(&format!("commit {idx}"), None);
        if idx < 3 {
            git.run_git(&[
                "tag",
                "-a",
                &format!("v0.{idx}.0"),
                "-m",
                &format!("Tag {idx}"),
            ]);
        }
    }
    for idx in 0..4 {
        ws.write_text(&format!("file_{idx}.rs"), &format!("// modified {idx}\n"));
    }
    for dirty in 0..5 {
        ws.write_text(&format!("dirty_{dirty}.txt"), "dirty\n");
    }

    let cfg = GitContextConfig {
        max_commits: 2,
        max_tags: 1,
        max_hotspots: 2,
        max_working_tree_lines: 2,
        max_diff_numstat_lines: 2,
        ..GitContextConfig::default()
    };
    let ctx = extract_git_context(ws.root(), &cfg).expect("capped context");

    assert_eq!(ctx.recent_commits.len(), 2);
    assert_eq!(ctx.recent_tags.len(), 1);
    assert_eq!(ctx.hotspots.len(), 2);
    assert_eq!(ctx.working_diff_numstat.len(), 2);
    assert_eq!(ctx.working_tree_lines.len(), 3);
    assert_eq!(ctx.working_tree_lines[2], "... (+7 more)");
}
