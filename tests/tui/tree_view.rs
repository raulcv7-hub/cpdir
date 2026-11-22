//! Main tree rendering, color weight tiers, folding, Win32 release guard, and narrow viewports.

use std::collections::HashMap;

use cpdir::application::{RunOutcome, WorkspaceSession};
use cpdir::domain::{
    DirectoryTree, NaturalExclusion, NaturalSelection, TextFileData, TextFileState,
    format_token_count,
};
use cpdir::git::GitContextConfig;
use cpdir::output::{XmlRenderOptions, render_strict_xml, render_strict_xml_with_options};
use cpdir::scanner::ScanConfig;
use cpdir::state::MemoryStore;
use cpdir::tokens::TokenCounter;
use cpdir::tui::AppExitAction;
use cpdir::tui::TuiAppState;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;
use ratatui::style::Color;

use crate::common::scan_helpers::{RecordingClipboard, scan_and_analyze};
use crate::common::tui_harness::{
    draw_and_dump, find_substring_fg, move_cursor_to, press_key, release_key,
};
use crate::common::workspace::{WorkspaceBuilder, sample_png_bytes};
use crate::common::xml_validator::parse_strict_xml;

#[test]
fn tui_tree_workflow_verifies_colors_folding_release_guard_and_final_xml() {
    let ws = WorkspaceBuilder::new();
    let row = "{\"id\":123,\"name\":\"alpha_beta\",\"active\":true},\n";
    ws.write_text(".gitignore", "ignored/\n");
    ws.write_text("src/main.rs", "fn main() { let ok = 1 < 2; }\r\n");
    ws.write_text("src/utils/helper.rs", "pub fn help() {}\n");
    ws.write_text("data/big_53k.json", &row.repeat(3_800));
    ws.write_text("ignored/secret.log", "ignored line\n");
    ws.write_bytes("assets/logo.png", &sample_png_bytes());
    ws.create_dir("empty_dir");
    ws.populate_files("node_modules/pkg", 15, "dep");

    let scan = scan_and_analyze(&ws);
    let mut state = TuiAppState::new(scan.tree, false);
    let mut terminal = Terminal::new(TestBackend::new(115, 28)).expect("test terminal");

    let initial = draw_and_dump(&mut terminal, &mut state);
    assert!(initial.contains("Tokens:"));
    assert!(!initial.contains("700.0k"));
    assert!(!initial.contains("Git:"));
    assert!(!initial.contains("[g] Git"));
    assert!(initial.contains("Path: OFF"));
    assert!(initial.contains("[p] Path"));
    assert!(initial.contains("Read: 3"));
    assert!(initial.contains("Unread: 2"));
    assert!(initial.contains("Discarded: 2"));
    assert!(!initial.contains("secret.log"));
    assert_eq!(
        find_substring_fg(&terminal, "main.rs"),
        Some(Color::Rgb(85, 220, 125))
    );
    assert_eq!(
        find_substring_fg(&terminal, "big_53k.json"),
        Some(Color::Rgb(130, 46, 46))
    );
    assert_eq!(
        find_substring_fg(&terminal, "logo.png"),
        Some(Color::Rgb(110, 115, 125))
    );

    release_key(&mut state, KeyCode::Char(' '));
    assert_eq!(state.metrics().counts.read_files, 3);

    move_cursor_to(&mut state, "src");
    press_key(&mut state, KeyCode::Char(' '));
    let after_toggle = draw_and_dump(&mut terminal, &mut state);
    assert!(after_toggle.contains("Read: 1"));
    assert_eq!(
        find_substring_fg(&terminal, "main.rs"),
        Some(Color::Rgb(42, 102, 60))
    );

    press_key(&mut state, KeyCode::Tab);
    assert!(!draw_and_dump(&mut terminal, &mut state).contains("helper.rs"));

    press_key(&mut state, KeyCode::Enter);
    assert_eq!(state.exit_action(), Some(AppExitAction::CopyToClipboard));

    let xml = render_strict_xml(&state.into_tree(), &scan.contents, None).expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    let summary = parsed.context_summary.expect("context_summary");
    assert_eq!(summary.included_files, 1);
    assert_eq!(summary.unselected_files, 4);
    assert_eq!(summary.discarded_files, 1);
    assert_eq!(summary.pruned_dirs.as_deref(), Some("node_modules"));

    let dir_tree = parsed.directory_structure.expect("directory_structure");
    assert!(dir_tree.contains("src/utils/helper.rs [unselected: off]"));
    assert!(dir_tree.contains("assets/logo.png [binary]"));
    assert!(dir_tree.contains("data/big_53k.json [unselected: >50k]"));
    assert!(dir_tree.contains("empty_dir/"));
    assert!(dir_tree.contains("ignored/secret.log [unselected: .gitignore]"));
    let emitted: Vec<String> = parsed.files.into_iter().map(|f| f.path).collect();
    assert_eq!(emitted, vec![".gitignore"]);
}

#[test]
fn tui_abs_path_toggle_updates_chrome_and_emits_escaped_root_path() {
    let mut tree = DirectoryTree::new("A&B");
    let main_id = tree
        .add_text_file(
            DirectoryTree::ROOT,
            "main.rs",
            TextFileData {
                byte_len: 12,
                tokens: 4,
                secret_hits: 0,
                state: TextFileState::new(NaturalSelection::Included),
            },
        )
        .expect("main.rs");
    let contents = HashMap::from([(main_id, "fn main() {}\n".to_owned())]);

    let mut state = TuiAppState::new(tree, false);
    let mut terminal = Terminal::new(TestBackend::new(100, 22)).expect("test terminal");

    assert!(!state.include_abs_path());
    assert!(draw_and_dump(&mut terminal, &mut state).contains("Path: OFF"));

    press_key(&mut state, KeyCode::Char('p'));
    assert!(state.include_abs_path());
    assert!(draw_and_dump(&mut terminal, &mut state).contains("Path: ON"));

    let abs = state
        .include_abs_path()
        .then_some(r"C:\Work\R&D <proj>\A&B");
    let xml = render_strict_xml_with_options(
        state.tree(),
        &contents,
        XmlRenderOptions {
            git_context: None,
            abs_root_path: abs,
        },
    )
    .expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    assert_eq!(
        parsed.context_summary.expect("summary").root_path,
        Some(r"C:\Work\R&D <proj>\A&B".to_owned())
    );

    press_key(&mut state, KeyCode::Char('p'));
    assert!(!state.include_abs_path());
    assert!(draw_and_dump(&mut terminal, &mut state).contains("Path: OFF"));
}

#[test]
fn tui_header_live_tokens_track_summary_structure_abs_path_and_clipboard_payload() {
    let ws = WorkspaceBuilder::new();
    let state_ws = WorkspaceBuilder::new();
    ws.write_text(".gitignore", "logs/\n");
    ws.write_text("src/main.rs", "fn main() {\r\n    let ok = 1 < 2;\r\n}\r\n");
    ws.write_text("logs/ignored.log", "ignored log entry\n");
    ws.write_bytes("assets/logo.png", &sample_png_bytes());
    ws.create_dir("empty_dir");
    ws.populate_files("node_modules/pkg", 5, "dep");

    let store = MemoryStore::in_dir(state_ws.root());
    let (mut session, mut state) = WorkspaceSession::open(
        ws.root(),
        store,
        ScanConfig::default(),
        GitContextConfig::default(),
    )
    .expect("open workspace");

    let counter = TokenCounter::o200k().expect("o200k");
    let mut terminal = Terminal::new(TestBackend::new(115, 24)).expect("test terminal");
    let initial_tokens = state.total_effective_tokens();
    assert!(initial_tokens > state.metrics().counts.selected_tokens);
    assert!(
        draw_and_dump(&mut terminal, &mut state)
            .contains(&format!("Tokens: {}", format_token_count(initial_tokens)))
    );

    // Unselect all files: header must match the exact XML summary+structure+envelope token count.
    press_key(&mut state, KeyCode::Char('a'));
    session.refresh_xml_overhead(&mut state);
    assert_eq!(state.metrics().counts.read_files, 0);
    let empty_live = state.total_effective_tokens();
    let empty_xml = render_strict_xml(state.tree(), session.contents(), None).expect("empty xml");
    assert_eq!(empty_live, counter.count(&empty_xml));

    // Toggle `[p]` ON and verify exact live token match for empty selection with `root_path`.
    press_key(&mut state, KeyCode::Char('p'));
    session.refresh_xml_overhead(&mut state);
    let empty_abs_live = state.total_effective_tokens();
    assert!(empty_abs_live > empty_live);
    let empty_abs_xml = render_strict_xml_with_options(
        state.tree(),
        session.contents(),
        XmlRenderOptions {
            git_context: None,
            abs_root_path: Some(session.display_abs_path()),
        },
    )
    .expect("empty abs xml");
    assert_eq!(empty_abs_live, counter.count(&empty_abs_xml));

    // Restore files and copy to clipboard.
    press_key(&mut state, KeyCode::Char('a'));
    session.refresh_xml_overhead(&mut state);
    let read_files = state.metrics().counts.read_files;
    let raw_selected = state.metrics().counts.selected_tokens;
    let pre_copy_live = state.total_effective_tokens();
    assert!(pre_copy_live > raw_selected);
    let header_screen = draw_and_dump(&mut terminal, &mut state);
    assert!(header_screen.contains(&format!("Tokens: {}", format_token_count(pre_copy_live))));

    state.request_copy();
    let clip = RecordingClipboard::default();
    let outcome = session.export_clipboard(state, &clip).expect("export");
    let copied_xml = &clip.copied.borrow()[0];
    let actual_xml_tokens = counter.count(copied_xml);

    assert!(matches!(
        outcome,
        RunOutcome::Copied { tokens, .. } if tokens == actual_xml_tokens
    ));
    assert!(pre_copy_live.abs_diff(actual_xml_tokens) <= read_files + 2);
}

#[test]
fn tui_responsive_layout_pins_badges_on_narrow_terminals_and_scrolls() {
    let mut tree = DirectoryTree::new("monorepo");
    let mut parent = DirectoryTree::ROOT;
    for level in ["apps", "web", "src", "features", "billing", "adapters"] {
        parent = tree.add_dir(parent, level, true).expect("nested dir");
    }
    tree.add_text_file(
        parent,
        "extremely_long_enterprise_controller_café_🚀.tsx",
        TextFileData {
            byte_len: 1_200,
            tokens: 420,
            secret_hits: 1,
            state: TextFileState::new(NaturalSelection::Excluded(NaturalExclusion::GitIgnore)),
        },
    )
    .expect("long nested file");

    for idx in 0..16 {
        tree.add_text_file(
            DirectoryTree::ROOT,
            format!("item_{idx:02}.rs"),
            TextFileData {
                byte_len: 200,
                tokens: 50,
                secret_hits: usize::from(idx == 0),
                state: TextFileState::new(NaturalSelection::Included),
            },
        )
        .expect("extra item");
    }

    let mut state = TuiAppState::new(tree, true);
    let mut terminal = Terminal::new(TestBackend::new(76, 18)).expect("narrow terminal");

    let screen = draw_and_dump(&mut terminal, &mut state);
    assert!(screen.contains("! Secrets: 1"));
    assert!(screen.contains("Git: ON"));
    assert!(screen.contains("Path: OFF"));
    assert!(screen.contains("[g] Git"));
    assert!(screen.contains("[p] Path"));
    assert!(screen.contains("[Esc] Quit"));
    assert!(screen.contains("… (420) [.gitignore] [!]"));
    assert!(screen.contains("├─") || screen.contains("└─"));

    for _ in 0..10 {
        press_key(&mut state, KeyCode::Down);
        let _ = draw_and_dump(&mut terminal, &mut state);
    }
    assert!(state.scroll_offset() > 0);
    assert!(state.cursor() < state.visible_rows().len());
}
