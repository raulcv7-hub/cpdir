//! High-token (>750k) end-to-end stress test and Windows filesystem/state resilience checks.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use cpdir::application::{AppError, RunOutcome, WorkspaceSession};
use cpdir::config::limits::DEFAULT_RESCAN_MAX_ENTRIES;
use cpdir::domain::{FileDiscardReason, InspectorTab};
use cpdir::git::GitContextConfig;
use cpdir::scanner::{ContentHydrationError, ScanConfig};
use cpdir::state::{MemoryStore, SavedWorkspaceState, StateError};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;

use crate::common::git_repo::GitRepoHarness;
use crate::common::scan_helpers::RecordingClipboard;
use crate::common::tui_harness::{draw_and_dump, press_key};
use crate::common::workspace::{WorkspaceBuilder, mid_stream_corrupt_utf8_bytes, sample_png_bytes};
use crate::common::xml_validator::parse_strict_xml;

fn build_stress_750k_workspace() -> WorkspaceBuilder {
    let ws = WorkspaceBuilder::new();
    let unit =
        "pub fn step_alpha_beta(x: usize) -> usize { x.wrapping_mul(31).wrapping_add(7) }\r\n";
    let body_11k = unit.repeat(500);
    let dump_55k = unit.repeat(2_500);
    let aws_key = ["AKIA", "1234567890ABCDEF"].concat();

    ws.write_text(".gitignore", "logs/\ncfg/.env\nnode_modules/\ntarget/\n");
    ws.write_text(".ignore", "!logs/keep.log\n");
    ws.write_text("logs/drop.log", "ignored log\n");
    ws.write_text("logs/keep.log", "rescued by .ignore\n");
    ws.write_text("cfg/.env", &format!("AWS_ACCESS_KEY_ID={aws_key}\n"));
    ws.write_text("data/dump_55k.json", &dump_55k);
    ws.write_bytes("src/fake_bin.rs", &sample_png_bytes());
    ws.write_bytes("src/corrupt.txt", &mid_stream_corrupt_utf8_bytes());
    ws.write_text("src/café_🚀.rs", &body_11k);

    for pkg in 0..10 {
        for shard in 0..7 {
            ws.write_text(&format!("pkg_{pkg}/mod_{shard}.rs"), &body_11k);
        }
    }
    ws.populate_files("node_modules/heavy", 150, "dep");
    ws.populate_files("target/debug/deps", 60, "obj");

    let git = GitRepoHarness::init(ws.root());
    git.commit_all("chore: stress seed", None);
    git.run_git(&["tag", "-a", "v0.1.0", "-m", "Stress baseline"]);
    ws.write_text("src/café_🚀.rs", &format!("{body_11k}// edited\r\n"));
    ws
}

#[test]
fn stress_750k_tokens_full_pipeline_tui_reingest_and_strict_xml() {
    let ws = build_stress_750k_workspace();
    let state_ws = WorkspaceBuilder::new();
    let store = MemoryStore::in_dir(state_ws.root());

    let (mut session, mut state) = WorkspaceSession::open(
        ws.root(),
        store,
        ScanConfig::default(),
        GitContextConfig::default(),
    )
    .expect("open 750k workspace");

    assert!(
        state.metrics().counts.selected_tokens > 750_000,
        "expected >750k tokens, got {}",
        state.metrics().counts.selected_tokens
    );
    assert_eq!(state.metrics().counts.active_secret_files, 0);
    assert_eq!(state.metrics().counts.total_secret_files, 1);

    let mut terminal = Terminal::new(TestBackend::new(120, 32)).expect("terminal");
    let screen = draw_and_dump(&mut terminal, &mut state);
    assert!(screen.contains("Tokens:"));
    assert!(screen.contains("Git: ON"));

    // Re-ingest `target` from Discarded and ensure `[a]` never activates it.
    state.open_inspector_tab(InspectorTab::Discarded);
    state.scroll_inspector(2);
    state.toggle_inspector_selection();
    assert!(session.reingest_pending(&mut state, DEFAULT_RESCAN_MAX_ENTRIES));
    state.close_modal();

    let baseline_files = state.metrics().counts.read_files;
    press_key(&mut state, KeyCode::Char('a'));
    assert_eq!(state.metrics().counts.read_files, 0);
    press_key(&mut state, KeyCode::Char('a'));
    assert_eq!(state.metrics().counts.read_files, baseline_files);

    state.request_copy();
    let clip = RecordingClipboard::default();
    let outcome = session
        .export_clipboard(state, &clip)
        .expect("export 750k xml");
    assert!(matches!(
        outcome,
        RunOutcome::Copied { tokens, .. } if tokens > 750_000
    ));

    let copied = clip.copied.borrow();
    let parsed = parse_strict_xml(&copied[0]).expect("750k xml parses strictly");
    assert!(parsed.has_git_context);
    assert!(parsed.files.iter().any(|f| f.path == "logs/keep.log"));
    assert!(parsed.files.iter().any(|f| f.path == "src/café_🚀.rs"));
    assert!(!parsed.files.iter().any(|f| f.path == "data/dump_55k.json"));
    assert!(
        parsed
            .files
            .iter()
            .all(|f| !f.path.starts_with("target/") && !f.path.starts_with("node_modules/"))
    );
    let dir_tree = parsed.directory_structure.as_deref().expect("dir tree");
    assert!(dir_tree.contains("target/"));
    assert!(!dir_tree.contains("node_modules"));
    assert!(!dir_tree.contains("heavy"));
    assert!(!copied[0].contains(&["AKIA", "1234567890ABCDEF"].concat()));
}

#[test]
fn resilience_handles_corrupt_memory_deleted_hydration_and_path_variants() {
    let ws = WorkspaceBuilder::new();
    let state_ws = WorkspaceBuilder::new();
    let store = MemoryStore::in_dir(state_ws.root());

    let large_ignored = "dropped large buffer\n".repeat(4_000);
    ws.write_text(".gitignore", "logs/\n");
    ws.write_text("src/main.rs", "fn main() {}\n");
    ws.write_text("logs/large.log", &large_ignored);

    let state_path = store.state_file_path(ws.root());
    fs::write(&state_path, "{corrupt").expect("write corrupt json");
    assert!(matches!(
        store.load(ws.root()),
        Err(StateError::CorruptJson(_))
    ));

    let future = SavedWorkspaceState {
        schema_version: 999,
        workspace_path: "x".to_owned(),
        overrides: BTreeMap::new(),
        git_context_enabled: None,
    };
    fs::write(&state_path, serde_json::to_string(&future).expect("json")).expect("write future");
    assert_eq!(
        store.load(ws.root()),
        Err(StateError::UnsupportedSchema(999))
    );

    if cfg!(windows) {
        let upper = PathBuf::from(ws.root().to_string_lossy().to_uppercase());
        assert_eq!(state_path, store.state_file_path(&upper));
    }

    // Lenient open degrades safely on corrupt/future schema, and deletion triggers clean error.
    let (mut session, mut state) = WorkspaceSession::open(
        ws.root(),
        store,
        ScanConfig::default(),
        GitContextConfig::default(),
    )
    .expect("open degrades safely");
    fs::remove_file(ws.root().join("logs/large.log")).expect("delete large.log");

    state.open_inspector_tab(InspectorTab::Unread);
    state.toggle_inspector_selection();
    state.close_modal();
    state.request_copy();

    let clip = RecordingClipboard::default();
    let err = session
        .export_clipboard(state, &clip)
        .expect_err("deleted large file must fail hydration cleanly");
    assert_eq!(
        err,
        AppError::ContentHydration(ContentHydrationError::FileUnavailable {
            rel_path: "logs/large.log".to_owned(),
            reason: FileDiscardReason::Unreadable,
        })
    );
}
