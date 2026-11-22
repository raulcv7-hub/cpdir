//! Self-hosting repository scan and realistic multi-stack project fixture integration tests.

use std::path::Path;

use cpdir::application::{RunOutcome, WorkspaceSession, analyze_tree_contents};
use cpdir::config::limits::DEFAULT_RESCAN_MAX_ENTRIES;
use cpdir::domain::{FileDiscardReason, InspectorTab, NodeKind};
use cpdir::git::GitContextConfig;
use cpdir::output::render_strict_xml;
use cpdir::scanner::{ScanConfig, rescan_pruned_dir};
use cpdir::state::MemoryStore;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;

use crate::common::scan_helpers::{RecordingClipboard, find_node_id, scan_and_analyze};
use crate::common::tui_harness::{draw_and_dump, press_key};
use crate::common::workspace::WorkspaceBuilder;
use crate::common::xml_validator::parse_strict_xml;

#[test]
fn self_host_scan_of_cpdir_repo_succeeds_with_clean_src_and_git_context() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let state_ws = WorkspaceBuilder::new();
    let store = MemoryStore::in_dir(state_ws.root());

    let (mut session, mut state) = WorkspaceSession::open(
        repo_root,
        store,
        ScanConfig::default(),
        GitContextConfig::default(),
    )
    .expect("self-host scan succeeds");

    let pruned: Vec<&str> = state
        .tree()
        .pruned_dirs()
        .iter()
        .map(|p| p.rel_path.as_str())
        .collect();
    assert!(pruned.contains(&".git"));
    if repo_root.join("target").is_dir() {
        assert!(pruned.contains(&"target"));
    }

    let src_secrets: Vec<_> = state
        .tree()
        .inspector_entries(state.metrics(), InspectorTab::Secrets)
        .into_iter()
        .filter(|e| e.rel_path.starts_with("src/"))
        .collect();
    assert_eq!(
        src_secrets,
        vec![],
        "unexpected secret false positive(s) inside src/"
    );

    let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("terminal");
    let screen = draw_and_dump(&mut terminal, &mut state);
    assert!(screen.contains("Tokens:"));

    if state.metrics().counts.active_secret_files > 0 {
        state.exclude_active_secret_files();
    }
    state.request_copy();

    let clip = RecordingClipboard::default();
    let outcome = session
        .export_clipboard(state, &clip)
        .expect("self-host xml export succeeds");
    assert!(matches!(outcome, RunOutcome::Copied { files, .. } if files > 10));

    let copied = clip.copied.borrow();
    let parsed = parse_strict_xml(&copied[0]).expect("self-host xml parses strictly");
    let dir_tree = parsed.directory_structure.as_deref().expect("dir tree");
    assert!(!dir_tree.contains("    target/"));
    assert!(
        parsed
            .files
            .iter()
            .all(|f| !f.path.starts_with("target/") && !f.path.starts_with(".git/"))
    );
    assert!(parsed.files.iter().any(|f| f.path == "Cargo.toml"));
    assert!(parsed.files.iter().any(|f| f.path == "src/lib.rs"));
}

#[test]
fn monorepo_rust_and_nextjs_fixture_prunes_deps_and_rescues_env_local() {
    let ws = WorkspaceBuilder::new();
    let stripe = ["sk_live", "_51H1234567890abcdefgh"].concat();
    let jsx = "export const Btn = () => <Tag attr=\"x & y\">ok && go</Tag>;\r\n";

    ws.write_text(".gitignore", "*.local\n");
    ws.write_text("Cargo.toml", "[workspace]\nmembers = [\"crates/core\"]\n");
    ws.write_text("Cargo.lock", &format!("checksum = \"{stripe}\"\n"));
    ws.write_text("crates/core/src/lib.rs", "pub fn ok() -> bool { true }\n");
    ws.write_text("apps/web/package.json", "{\"name\":\"web\"}\n");
    ws.write_text("apps/web/pnpm-lock.yaml", &format!("integrity: {stripe}\n"));
    ws.write_text("apps/web/.ignore", "!.env.local\n");
    ws.write_text("apps/web/.env.local", &format!("STRIPE_KEY={stripe}\n"));
    ws.write_text("apps/web/src/Button.tsx", jsx);
    ws.write_text("apps/web/src/types.d.ts", "export interface Props {}\n");
    ws.populate_files("apps/web/.next/static", 12, "chunk");
    ws.populate_files("apps/web/node_modules/react", 20, "mod");

    let scan = scan_and_analyze(&ws);
    let metrics = scan.tree.compute_metrics();
    assert_eq!(metrics.counts.total_secret_files, 1);
    assert_eq!(metrics.counts.active_secret_files, 1);

    let env_id = find_node_id(&scan.tree, "apps/web/.env.local");
    assert!(matches!(
        &scan.tree.get(env_id).expect("env").kind,
        NodeKind::TextFile(f) if f.state.is_selected() && f.secret_hits == 1
    ));

    let xml = render_strict_xml(&scan.tree, &scan.contents, None).expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    assert!(!xml.contains("node_modules"));
    assert!(!xml.contains(".next"));
    assert!(
        parsed
            .files
            .iter()
            .any(|f| f.path == "apps/web/src/Button.tsx" && f.content == jsx)
    );
}

#[test]
fn python_ml_and_go_cpp_bom_fixtures_enforce_thresholds_and_contextual_pruning() {
    let ws = WorkspaceBuilder::new();
    let nb_cell = "{\"cell_type\":\"code\",\"source\":[\"x = alpha_1 + beta_2\"]},\n";
    let huge_nb = format!("{{\"cells\":[{}]}}", nb_cell.repeat(4_200));
    let mut bom_cpp = vec![0xEF, 0xBB, 0xBF];
    bom_cpp.extend_from_slice("int main() { return 0; }\r\n".as_bytes());

    ws.populate_files(".venv/lib/site-packages", 10, "py");
    ws.populate_files("__pycache__", 5, "pyc");
    ws.write_text("poetry.lock", "hash = \"abc\"\n");
    ws.write_text("train.ipynb", &huge_nb);
    ws.write_bytes("big_dataset.csv", &vec![b'1'; 5 * 1024 * 1024 + 64]);
    ws.write_text("src/build/generated.cpp", "// generated inside src\n");
    ws.write_bytes("win_bom.cpp", &bom_cpp);
    ws.write_text("vendor/pkg/mod.go", "package pkg\n");

    let scan = scan_and_analyze(&ws);
    let nb_id = find_node_id(&scan.tree, "train.ipynb");
    assert!(matches!(
        &scan.tree.get(nb_id).expect("nb").kind,
        NodeKind::TextFile(f) if !f.state.is_selected()
    ));

    let csv_id = find_node_id(&scan.tree, "big_dataset.csv");
    assert!(matches!(
        &scan.tree.get(csv_id).expect("csv").kind,
        NodeKind::DiscardedFile(FileDiscardReason::ExceedsHardCap { .. })
    ));

    let mut state = cpdir::tui::TuiAppState::new(scan.tree, false);
    let mut contents = scan.contents;
    let mut sub = rescan_pruned_dir(
        ws.root(),
        "vendor",
        &ScanConfig::default(),
        DEFAULT_RESCAN_MAX_ENTRIES,
    )
    .expect("rescan vendor");
    analyze_tree_contents(&mut sub.tree, &sub.contents).expect("analyze vendor");

    let outcome = state
        .apply_reingested_dir("vendor", sub.tree, sub.truncated)
        .expect("merge vendor");
    for (old_id, new_id) in outcome.id_map {
        if let Some(text) = sub.contents.remove(&old_id) {
            contents.insert(new_id, text);
        }
    }

    press_key(&mut state, KeyCode::Char('a'));
    assert_eq!(state.metrics().counts.read_files, 0);
    press_key(&mut state, KeyCode::Char('a'));
    assert_eq!(state.metrics().counts.read_files, 3);

    let xml = render_strict_xml(state.tree(), &contents, None).expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    let paths: Vec<String> = parsed.files.into_iter().map(|f| f.path).collect();
    assert_eq!(
        paths,
        vec!["src/build/generated.cpp", "poetry.lock", "win_bom.cpp"]
    );
}
