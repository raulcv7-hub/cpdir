//! Real Git repository history, UTF-8 path, unborn-repo, and malicious `.git/config` tests.

mod common;

use std::collections::HashMap;
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use common::git_repo::GitRepoHarness;
use common::workspace::WorkspaceBuilder;
use common::xml_validator::parse_strict_xml;
use cpdir::domain::DirectoryTree;
use cpdir::git::{GitContextConfig, extract_git_context};
use cpdir::output::render_strict_xml;
use cpdir::scanner::{ScanConfig, scan_directory};
use cpdir::tui::TuiAppState;

#[test]
fn real_git_repo_extracts_commits_tags_hotspots_utf8_paths_and_toggles_xml() {
    let ws = WorkspaceBuilder::new();
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

    let ctx = extract_git_context(ws.root(), &GitContextConfig::default())
        .expect("git context must be detected");
    assert!(ctx.branch.contains("main"));
    assert!(
        ctx.working_tree_lines
            .iter()
            .any(|l| l.contains("M src/main.rs"))
    );
    assert!(ctx.working_tree_lines.iter().any(|l| l == "?? notes.txt"));

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

    let scan = scan_directory(ws.root(), &ScanConfig::default()).expect("scan");
    let mut ui = TuiAppState::new(scan.tree, true);
    assert!(ui.include_git_context());

    let xml_on = render_strict_xml(ui.tree(), &scan.contents, Some(&ctx)).expect("xml on");
    let parsed_on = parse_strict_xml(&xml_on).expect("valid xml with git");
    assert!(parsed_on.has_git_context);
    assert!(xml_on.contains("<recent_tags count=\"2\">"));
    assert!(xml_on.contains("src/café.rs (1 commits)"));

    ui.toggle_git_context();
    assert!(!ui.include_git_context());
    let xml_off = render_strict_xml(ui.tree(), &scan.contents, None).expect("xml off");
    let parsed_off = parse_strict_xml(&xml_off).expect("valid xml without git");
    assert!(!parsed_off.has_git_context);
}

#[test]
fn handles_non_git_directories_and_unborn_repositories_gracefully() {
    let non_git = WorkspaceBuilder::new();
    non_git.write_text("hello.rs", "fn hi() {}\n");
    assert_eq!(
        extract_git_context(non_git.root(), &GitContextConfig::default()),
        None
    );

    let unborn = WorkspaceBuilder::new();
    unborn.write_text("draft.txt", "uncommitted\n");
    let _git = GitRepoHarness::init(unborn.root());

    let ctx = extract_git_context(unborn.root(), &GitContextConfig::default())
        .expect("unborn repo still returns context");
    assert!(ctx.branch.contains("main"));
    assert_eq!(ctx.working_tree_lines, vec!["?? draft.txt"]);
    assert_eq!(ctx.recent_commits, vec![]);
    assert_eq!(ctx.recent_tags, vec![]);
    assert_eq!(ctx.hotspots, vec![]);

    let empty_contents: HashMap<_, String> = HashMap::new();
    let xml = render_strict_xml(&DirectoryTree::new("unborn"), &empty_contents, Some(&ctx))
        .expect("xml renders unborn repo");
    assert!(parse_strict_xml(&xml).expect("valid xml").has_git_context);
}

#[test]
fn neutralizes_malicious_git_config_hooks_verified_by_positive_control() {
    let ws = WorkspaceBuilder::new();
    let trap_ws = WorkspaceBuilder::new();
    ws.write_text("app.rs", "fn app() {}\n");
    let git = GitRepoHarness::init(ws.root());
    git.commit_all("feat: base", None);
    inject_dummy_gpg_signature(ws.root(), &git);

    let pwned_gpg = trap_ws.root().join("pwned_gpg.txt");
    let gpg_script = create_gpg_trap_script(trap_ws.root(), &pwned_gpg);
    let gpg_program = gpg_script.to_string_lossy().replace('\\', "/");
    git.set_local_config("log.showSignature", "true");
    git.set_local_config("gpg.program", &gpg_program);
    git.append_raw_git_config("[core]\n\tfsmonitor = echo pwned > pwned_fsmon.txt #\n");

    let ctx = extract_git_context(ws.root(), &GitContextConfig::default())
        .expect("hardened extraction succeeds");
    assert_eq!(ctx.recent_commits.len(), 1);
    assert!(
        !ws.root().join("pwned_fsmon.txt").exists(),
        "hardened git must neutralize core.fsmonitor"
    );
    assert!(
        !pwned_gpg.exists(),
        "hardened git must neutralize gpg.program"
    );

    // Positive control: unhardened git commands execute both traps.
    let _ = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(ws.root())
        .output()
        .expect("run unhardened git status");
    assert!(
        ws.root().join("pwned_fsmon.txt").exists(),
        "positive control: unhardened git status must trigger core.fsmonitor"
    );

    let _ = Command::new("git")
        .args(["log", "-1", "--format=%h"])
        .current_dir(ws.root())
        .output()
        .expect("run unhardened git log");
    assert!(
        pwned_gpg.exists(),
        "positive control: unhardened git log must trigger gpg.program"
    );
}

#[test]
fn enforces_configured_caps_for_commits_tags_hotspots_and_working_tree() {
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
    for dirty in 0..5 {
        ws.write_text(&format!("dirty_{dirty}.txt"), "dirty\n");
    }

    let cfg = GitContextConfig {
        max_commits: 2,
        max_tags: 1,
        max_hotspots: 2,
        max_working_tree_lines: 2,
        ..GitContextConfig::default()
    };
    let ctx = extract_git_context(ws.root(), &cfg).expect("capped context");

    assert_eq!(ctx.recent_commits.len(), 2);
    assert_eq!(ctx.recent_tags.len(), 1);
    assert_eq!(ctx.hotspots.len(), 2);
    assert_eq!(ctx.working_tree_lines.len(), 3);
    assert_eq!(ctx.working_tree_lines[2], "... (+3 more)");
}

fn inject_dummy_gpg_signature(repo_root: &std::path::Path, git: &GitRepoHarness<'_>) {
    let out = Command::new("git")
        .args(["cat-file", "commit", "HEAD"])
        .current_dir(repo_root)
        .output()
        .expect("read HEAD commit object");
    assert!(out.status.success());

    let raw = String::from_utf8(out.stdout).expect("utf8 commit object");
    let (headers, body) = raw
        .split_once("\n\n")
        .expect("commit object has header/body separator");
    let signed = format!(
        "{headers}\ngpgsig -----BEGIN PGP SIGNATURE-----\n dummy\n -----END PGP SIGNATURE-----\n\n{body}"
    );

    let mut child = Command::new("git")
        .args(["hash-object", "-t", "commit", "-w", "--stdin"])
        .current_dir(repo_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn git hash-object");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(signed.as_bytes())
        .expect("write signed commit");
    let hashed = child.wait_with_output().expect("wait hash-object");
    assert!(hashed.status.success());

    let new_oid = String::from_utf8_lossy(&hashed.stdout).trim().to_owned();
    git.run_git(&["update-ref", "refs/heads/main", &new_oid]);
}

fn create_gpg_trap_script(dir: &std::path::Path, marker_file: &std::path::Path) -> PathBuf {
    let script_path = dir.join("trap_gpg.cmd");
    let body = format!(
        "@echo off\r\necho pwned > \"{}\"\r\n",
        marker_file.display()
    );
    fs::write(&script_path, body).expect("write trap_gpg.cmd");
    script_path
}
