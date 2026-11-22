//! Unified inspector tab cycling, in-place toggles, jump-to-tree, and Bidi sanitization tests.

use cpdir::domain::{DirectoryTree, InspectorTab, NaturalSelection, TextFileData, TextFileState};
use cpdir::tui::{TuiAppState, UiMode};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;

use crate::common::scan_helpers::scan_and_analyze;
use crate::common::tui_harness::{draw_and_dump, press_key};
use crate::common::workspace::{WorkspaceBuilder, mid_stream_corrupt_utf8_bytes, sample_png_bytes};

#[test]
fn tui_unified_inspector_tabs_toggle_and_jump_to_collapsed_folder() {
    let ws = WorkspaceBuilder::new();
    ws.write_text(".gitignore", "*.log\nlogs/\n");
    ws.write_text("src/app.rs", "pub fn run() {}\n");
    ws.write_text("logs/debug.log", "ignored log 1\n");
    ws.write_text("logs/trace.log", "ignored log 2\n");
    ws.write_bytes("assets/logo.png", &sample_png_bytes());
    ws.write_bytes("broken.rs", &mid_stream_corrupt_utf8_bytes());
    ws.populate_files("node_modules/dep", 5, "dep");

    let scan = scan_and_analyze(&ws);
    let mut state = TuiAppState::new(scan.tree, false);
    let mut terminal = Terminal::new(TestBackend::new(105, 24)).expect("test terminal");

    press_key(&mut state, KeyCode::Char('d'));
    assert_eq!(
        state.mode(),
        UiMode::Inspector {
            tab: InspectorTab::Discarded,
            scroll: 0
        }
    );
    let disc_view = draw_and_dump(&mut terminal, &mut state);
    assert!(disc_view.contains("Discarded (3)"));
    assert!(disc_view.contains("[PRUNED]"));
    assert!(disc_view.contains("[BIN]"));
    assert!(disc_view.contains("[UTF8]"));

    press_key(&mut state, KeyCode::Char('n'));
    let unread_view = draw_and_dump(&mut terminal, &mut state);
    assert!(unread_view.contains("Unread (2)"));
    assert!(unread_view.contains("[DIR] logs/ (2 files"));

    // Toggle entire ignored `logs/` folder ON straight from the Unread inspector via Space.
    press_key(&mut state, KeyCode::Char(' '));
    assert_eq!(state.metrics().counts.read_files, 4);
    assert_eq!(state.metrics().counts.unread_files, 0);

    // Move to `logs/trace.log` inside Unread and press Enter -> expands `logs/` and jumps cursor.
    press_key(&mut state, KeyCode::Down);
    press_key(&mut state, KeyCode::Down);
    press_key(&mut state, KeyCode::Enter);
    assert_eq!(state.mode(), UiMode::Tree);
    let landed = state.visible_rows()[state.cursor()];
    assert_eq!(
        state.tree().nodes()[landed.node_id.index()].rel_path,
        "logs/trace.log"
    );

    press_key(&mut state, KeyCode::Char('i'));
    press_key(&mut state, KeyCode::Tab);
    press_key(&mut state, KeyCode::Tab);
    assert!(matches!(
        state.mode(),
        UiMode::Inspector {
            tab: InspectorTab::Secrets,
            ..
        }
    ));
    press_key(&mut state, KeyCode::Left);
    assert!(matches!(
        state.mode(),
        UiMode::Inspector {
            tab: InspectorTab::Discarded,
            ..
        }
    ));
    press_key(&mut state, KeyCode::Esc);
    assert_eq!(state.mode(), UiMode::Tree);

    press_key(&mut state, KeyCode::Char('a'));
    assert_eq!(state.metrics().counts.read_files, 2);
    press_key(&mut state, KeyCode::Char('a'));
    assert_eq!(state.metrics().counts.read_files, 0);
    press_key(&mut state, KeyCode::Char('a'));
    assert_eq!(state.metrics().counts.read_files, 2);

    let mut bidi_tree = DirectoryTree::new("repo");
    bidi_tree
        .add_text_file(
            DirectoryTree::ROOT,
            "evil\u{202E}gpj.rs\x1B",
            TextFileData {
                byte_len: 10,
                tokens: 5,
                secret_hits: 0,
                state: TextFileState::new(NaturalSelection::Included),
            },
        )
        .expect("add bidi");
    let mut bidi_state = TuiAppState::new(bidi_tree, false);
    let bidi_screen = draw_and_dump(&mut terminal, &mut bidi_state);
    assert!(bidi_screen.contains("evil\u{FFFD}gpj.rs\u{FFFD}"));
    assert!(!bidi_screen.contains('\u{202E}'));
    assert!(!bidi_screen.contains('\x1B'));
}
