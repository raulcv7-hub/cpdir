//! Multi-run delta persistence, disk mutation evolution, and corrupt-state recovery tests.

use std::fs;
use std::path::PathBuf;

use cpdir::output::render_strict_xml;
use cpdir::state::{MemoryStore, SavedWorkspaceState, StateError, normalize_dir_for_hash};
use cpdir::tui::TuiAppState;

use crate::common::scan_helpers::{find_node_id, scan_and_analyze};
use crate::common::workspace::WorkspaceBuilder;
use crate::common::xml_validator::parse_strict_xml;

#[test]
fn multi_run_disk_mutation_preserves_deltas_and_auto_includes_new_files() {
    let ws = WorkspaceBuilder::new();
    let state_ws = WorkspaceBuilder::new();
    let store = MemoryStore::in_dir(state_ws.root());
    let row = "{\"id\":123,\"name\":\"alpha_beta\",\"active\":true},\n";

    ws.write_text(".gitignore", "ignored.log\nlogs/\n");
    ws.write_text("src/main.rs", "fn main() {}\n");
    ws.write_text("src/lib.rs", "pub fn lib_fn() {}\n");
    ws.write_text("docs/readme.md", "# Readme\n");
    ws.write_text("docs/old.md", "# Old file to delete\n");
    ws.write_text("ignored.log", "rescued by user in run 1\n");
    ws.write_text("logs/debug.log", "ignored log\n");
    ws.write_text("data/big_53k.json", &row.repeat(3_800));

    let scan1 = scan_and_analyze(&ws);
    let mut tree1 = scan1.tree;
    tree1.toggle_selection(find_node_id(&tree1, "docs"));
    tree1.toggle_selection(find_node_id(&tree1, "src/lib.rs"));
    tree1.toggle_selection(find_node_id(&tree1, "ignored.log"));
    tree1.toggle_selection(find_node_id(&tree1, "data/big_53k.json"));

    let mut ui1 = TuiAppState::new(tree1, true);
    ui1.toggle_git_context();
    assert!(!ui1.include_git_context());

    let saved1 = SavedWorkspaceState::new(
        normalize_dir_for_hash(ws.root()),
        ui1.tree().collect_overrides(),
        Some(ui1.include_git_context()),
    );
    store.save(ws.root(), &saved1).expect("save run 1");

    ws.write_text("src/new_module.rs", "pub fn brand_new() {}\n");
    ws.write_text("docs/new_draft.md", "# New draft inside excluded docs\n");
    ws.write_text("logs/new_trace.log", "new ignored trace\n");
    fs::remove_file(ws.root().join("docs/old.md")).expect("remove old.md");

    let mut scan2 = scan_and_analyze(&ws);
    let loaded = store
        .load(ws.root())
        .expect("valid json")
        .expect("saved state present");
    scan2.tree.apply_overrides(&loaded.overrides);

    let mut ui2 = TuiAppState::new(scan2.tree, true);
    if let Some(git_on) = loaded.git_context_enabled {
        ui2.set_include_git_context(git_on);
    }
    assert!(!ui2.include_git_context());

    let xml = render_strict_xml(ui2.tree(), &scan2.contents, None).expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    let paths: Vec<String> = parsed.files.into_iter().map(|f| f.path).collect();

    assert_eq!(
        paths,
        vec![
            "data/big_53k.json",
            "src/main.rs",
            "src/new_module.rs",
            ".gitignore",
            "ignored.log",
        ]
    );
}

#[test]
fn nested_unexclusion_survives_disk_reload_and_select_all_cleans_state_file() {
    let ws = WorkspaceBuilder::new();
    let state_ws = WorkspaceBuilder::new();
    let store = MemoryStore::in_dir(state_ws.root());

    ws.write_text("parent/a.rs", "fn a() {}\n");
    ws.write_text("parent/sub/b.rs", "fn b() {}\n");

    let scan1 = scan_and_analyze(&ws);
    let mut tree1 = scan1.tree;
    tree1.toggle_selection(find_node_id(&tree1, "parent"));
    tree1.toggle_selection(find_node_id(&tree1, "parent/sub"));

    let state1 = SavedWorkspaceState::new(
        normalize_dir_for_hash(ws.root()),
        tree1.collect_overrides(),
        None,
    );
    store.save(ws.root(), &state1).expect("save nested state");

    ws.write_text("parent/new_sibling.rs", "fn sibling() {}\n");
    ws.write_text("parent/sub/new_child.rs", "fn child() {}\n");

    let mut scan2 = scan_and_analyze(&ws);
    let loaded2 = store.load(ws.root()).expect("load").expect("present");
    scan2.tree.apply_overrides(&loaded2.overrides);

    let xml = render_strict_xml(&scan2.tree, &scan2.contents, None).expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    let paths: Vec<String> = parsed.files.into_iter().map(|f| f.path).collect();
    assert_eq!(paths, vec!["parent/sub/b.rs", "parent/sub/new_child.rs"]);

    scan2.tree.select_all_default();
    let state3 = SavedWorkspaceState::new(
        normalize_dir_for_hash(ws.root()),
        scan2.tree.collect_overrides(),
        None,
    );
    assert!(state3.is_default_empty());
    store.save(ws.root(), &state3).expect("clean state file");
    assert!(!store.state_file_path(ws.root()).exists());
    assert_eq!(store.load(ws.root()), Ok(None));
}

#[test]
fn normalizes_windows_path_variants_and_degrades_safely_on_corrupt_or_future_schemas() {
    let ws = WorkspaceBuilder::new();
    let state_ws = WorkspaceBuilder::new();
    let store = MemoryStore::in_dir(state_ws.root());

    let canonical = fs::canonicalize(ws.root()).expect("canonical path");
    let dot_joined = ws.root().join(".");

    let path_base = store.state_file_path(ws.root());
    assert_eq!(path_base, store.state_file_path(&canonical));
    assert_eq!(path_base, store.state_file_path(&dot_joined));

    if cfg!(windows) {
        let upper = PathBuf::from(ws.root().to_string_lossy().to_uppercase());
        assert_eq!(path_base, store.state_file_path(&upper));
    }

    let saved = SavedWorkspaceState::new("demo", [("a.rs".to_owned(), false)].into(), None);
    store.save(ws.root(), &saved).expect("save");
    let leftovers: Vec<_> = fs::read_dir(state_ws.root())
        .expect("read state dir")
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
        .collect();
    assert_eq!(leftovers.len(), 0);

    fs::write(&path_base, "{truncated json").expect("write corrupt");
    assert!(matches!(
        store.load(ws.root()),
        Err(StateError::CorruptJson(_))
    ));
    assert_eq!(store.load_lenient(ws.root()), None);

    fs::write(
        &path_base,
        r#"{"schema_version":99,"workspace_path":"x","overrides":{}}"#,
    )
    .expect("write future schema");
    assert_eq!(
        store.load(ws.root()),
        Err(StateError::UnsupportedSchema(99))
    );
    assert_eq!(store.load_lenient(ws.root()), None);
}
