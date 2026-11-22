//! Adversarial `.git/config` hook neutralization tests verified with positive controls.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cpdir::git::{GitContextConfig, extract_git_context};

use crate::common::git_repo::GitRepoHarness;
use crate::common::workspace::WorkspaceBuilder;

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

fn inject_dummy_gpg_signature(repo_root: &Path, git: &GitRepoHarness<'_>) {
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

fn create_gpg_trap_script(dir: &Path, marker_file: &Path) -> PathBuf {
    let script_path = dir.join("trap_gpg.cmd");
    let body = format!(
        "@echo off\r\necho pwned > \"{}\"\r\n",
        marker_file.display()
    );
    fs::write(&script_path, body).expect("write trap_gpg.cmd");
    script_path
}
