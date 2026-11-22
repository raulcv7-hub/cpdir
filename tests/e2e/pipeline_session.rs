//! End-to-end orchestration tests for `WorkspaceSession` without requiring a physical TTY.

use std::cell::RefCell;
use std::fs;

use cpdir::application::{AppError, RunOutcome, WorkspaceSession};
use cpdir::config::limits::DEFAULT_RESCAN_MAX_ENTRIES;
use cpdir::domain::{FileDiscardReason, InspectorTab};
use cpdir::git::GitContextConfig;
use cpdir::output::{ClipboardError, ClipboardWriter};
use cpdir::scanner::{ContentHydrationError, ScanConfig, ScanError};
use cpdir::state::MemoryStore;

use crate::common::git_repo::GitRepoHarness;
use crate::common::workspace::WorkspaceBuilder;
use crate::common::xml_validator::parse_strict_xml;

#[derive(Default)]
struct RecordingClipboard {
    copied: RefCell<Vec<String>>,
    fail_with: Option<ClipboardError>,
}

impl ClipboardWriter for RecordingClipboard {
    fn copy_text(&self, text: &str) -> Result<(), ClipboardError> {
        if let Some(err) = &self.fail_with {
            return Err(err.clone());
        }
        self.copied.borrow_mut().push(text.to_owned());
        Ok(())
    }
}

#[test]
fn workspace_session_open_reingest_export_and_memory_roundtrip() {
    let ws = WorkspaceBuilder::new();
    let state_ws = WorkspaceBuilder::new();
    ws.write_text(".gitignore", "ignored.log\n");
    ws.write_text("src/main.rs", "fn main() {}\n");
    ws.write_text("ignored.log", "ignored line\n");
    ws.write_text("vendor/dep.rs", "pub fn dep() {}\n");

    let git = GitRepoHarness::init(ws.root());
    git.commit_all("chore: initial", None);

    let store = MemoryStore::in_dir(state_ws.root());
    let (mut session, mut state) = WorkspaceSession::open(
        ws.root(),
        store.clone(),
        ScanConfig::default(),
        GitContextConfig::default(),
    )
    .expect("open workspace");

    assert!(state.has_git_context());
    assert!(state.git_context_tokens() > 0);
    assert_eq!(state.metrics().counts.read_files, 2);

    state.open_inspector_tab(InspectorTab::Discarded);
    state.scroll_inspector(1);
    state.toggle_inspector_selection();
    assert!(session.reingest_pending(&mut state, DEFAULT_RESCAN_MAX_ENTRIES));
    assert!(
        state
            .tree()
            .nodes()
            .iter()
            .any(|n| n.rel_path == "vendor/dep.rs")
    );

    state.open_inspector_tab(InspectorTab::Unread);
    state.scroll_inspector(2);
    state.toggle_inspector_selection();
    state.close_modal();

    state.toggle_git_context();
    state.request_copy();

    let clipboard = RecordingClipboard::default();
    let outcome = session
        .export_clipboard(state, &clipboard)
        .expect("export succeeds");
    assert!(matches!(outcome, RunOutcome::Copied { files: 3, .. }));

    let copied = clipboard.copied.borrow();
    assert_eq!(copied.len(), 1);
    let parsed = parse_strict_xml(&copied[0]).expect("valid strict xml");
    assert!(!parsed.has_git_context);
    let paths: Vec<String> = parsed.files.into_iter().map(|f| f.path).collect();
    assert_eq!(paths, vec!["src/main.rs", "vendor/dep.rs", ".gitignore"]);

    ws.write_text("src/added.rs", "pub fn added() {}\n");
    let (_session2, state2) = WorkspaceSession::open(
        ws.root(),
        store,
        ScanConfig::default(),
        GitContextConfig::default(),
    )
    .expect("reopen workspace");
    assert!(!state2.include_git_context());
    assert_eq!(state2.metrics().counts.read_files, 3);
}

#[test]
fn workspace_session_cancel_and_hydration_failure_leave_clipboard_untouched() {
    let ws = WorkspaceBuilder::new();
    let state_ws = WorkspaceBuilder::new();
    let large_ignored = "ignored line payload\n".repeat(4_000);
    ws.write_text(".gitignore", "logs/\n");
    ws.write_text("src/main.rs", "fn main() {}\n");
    ws.write_text("logs/big.log", &large_ignored);

    let store = MemoryStore::in_dir(state_ws.root());
    let (mut session, mut state) = WorkspaceSession::open(
        ws.root(),
        store.clone(),
        ScanConfig::default(),
        GitContextConfig::default(),
    )
    .expect("open workspace");

    state.cancel_exit();
    let clipboard = RecordingClipboard::default();
    assert_eq!(
        session
            .export_clipboard(state, &clipboard)
            .expect("cancel ok"),
        RunOutcome::Cancelled
    );
    assert_eq!(clipboard.copied.borrow().len(), 0);
    assert!(!store.state_file_path(ws.root()).exists());

    let (mut session2, mut state2) = WorkspaceSession::open(
        ws.root(),
        store,
        ScanConfig::default(),
        GitContextConfig::default(),
    )
    .expect("reopen workspace");
    assert_eq!(session2.contents().len(), 2);

    fs::remove_file(ws.root().join("logs/big.log")).expect("remove big.log");
    state2.open_inspector_tab(InspectorTab::Unread);
    state2.toggle_inspector_selection();
    state2.close_modal();
    state2.request_copy();

    let err = session2
        .export_clipboard(state2, &clipboard)
        .expect_err("missing released file must fail hydration");
    assert_eq!(
        err,
        AppError::ContentHydration(ContentHydrationError::FileUnavailable {
            rel_path: "logs/big.log".to_owned(),
            reason: FileDiscardReason::Unreadable,
        })
    );
    assert_eq!(clipboard.copied.borrow().len(), 0);
}

#[test]
fn workspace_session_surfaces_non_directory_and_clipboard_errors() {
    let ws = WorkspaceBuilder::new();
    let state_ws = WorkspaceBuilder::new();
    let plain = ws.write_text("not_a_dir.txt", "hello");
    let store = MemoryStore::in_dir(state_ws.root());

    let err = WorkspaceSession::open(
        &plain,
        store.clone(),
        ScanConfig::default(),
        GitContextConfig::default(),
    )
    .expect_err("must reject file target");
    assert!(matches!(err, AppError::Scan(ScanError::NotADirectory(_))));

    ws.write_text("app.rs", "fn app() {}\n");
    let (mut session, mut state) = WorkspaceSession::open(
        ws.root(),
        store.clone(),
        ScanConfig::default(),
        GitContextConfig::default(),
    )
    .expect("open ws");
    state.request_copy();

    let failing_clip = RecordingClipboard {
        copied: RefCell::new(Vec::new()),
        fail_with: Some(ClipboardError::WriteFailed("locked".to_owned())),
    };
    let clip_err = session
        .export_clipboard(state, &failing_clip)
        .expect_err("clipboard error surfaces");
    assert_eq!(
        clip_err,
        AppError::Clipboard(ClipboardError::WriteFailed("locked".to_owned()))
    );
    assert!(!store.state_file_path(ws.root()).exists());
}
