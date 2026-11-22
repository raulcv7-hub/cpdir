//! Mouse wheel scrolling, tree row/fold clicks, and inspector tab/scroll-sync click tests.

use cpdir::domain::{
    DirectoryTree, InspectorTab, NaturalExclusion, NaturalSelection, NodeKind, TextFileData,
    TextFileState,
};
use cpdir::tui::{TuiAppState, TuiViewportAreas, UiMode, handle_mouse_event};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyModifiers, MouseEvent, MouseEventKind};

use crate::common::scan_helpers::scan_and_analyze;
use crate::common::tui_harness::{click_left, draw_screen_with_layout, press_key};
use crate::common::workspace::WorkspaceBuilder;

#[test]
fn tui_mouse_wheel_and_tree_click_navigation() {
    let ws = WorkspaceBuilder::new();
    ws.write_text("src/main.rs", "fn main() {}\n");
    ws.write_text("src/lib.rs", "pub fn lib() {}\n");
    ws.write_text("docs/readme.md", "# readme\n");

    let scan = scan_and_analyze(&ws);
    let mut state = TuiAppState::new(scan.tree, false);
    let mut layout = TuiViewportAreas::default();
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).expect("test terminal");

    draw_screen_with_layout(&mut terminal, &mut state, &mut layout);
    assert_eq!(state.cursor(), 0);
    assert_eq!(state.metrics().counts.read_files, 3);

    let changed = handle_mouse_event(
        &mut state,
        &layout,
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 10,
            row: 5,
            modifiers: KeyModifiers::NONE,
        },
    );
    assert!(changed);
    assert_eq!(state.cursor(), 2);

    let target_row_y = layout.tree_body.y + 2;
    assert!(click_left(&mut state, &layout, 15, target_row_y));
    draw_screen_with_layout(&mut terminal, &mut state, &mut layout);
    assert_eq!(state.cursor(), 1);
    assert_eq!(state.metrics().counts.read_files, 2);

    let fold_col = layout.tree_body.x + 5;
    let dir_row_y = layout.tree_body.y + 2;
    assert!(click_left(&mut state, &layout, fold_col, dir_row_y));
    draw_screen_with_layout(&mut terminal, &mut state, &mut layout);
    assert_eq!(state.visible_rows().len(), 5);

    assert!(click_left(&mut state, &layout, fold_col, dir_row_y));
    draw_screen_with_layout(&mut terminal, &mut state, &mut layout);
    assert_eq!(state.visible_rows().len(), 6);
}

#[test]
fn tui_mouse_inspector_tab_and_row_clicks() {
    let ws = WorkspaceBuilder::new();
    ws.write_text("src/main.rs", "fn main() {}\n");
    ws.write_text("src/lib.rs", "pub fn lib() {}\n");
    ws.write_text("docs/readme.md", "# readme\n");

    let scan = scan_and_analyze(&ws);
    let mut state = TuiAppState::new(scan.tree, false);
    let mut layout = TuiViewportAreas::default();
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).expect("test terminal");

    draw_screen_with_layout(&mut terminal, &mut state, &mut layout);
    let docs_row_y = layout.tree_body.y + 2;
    assert!(click_left(&mut state, &layout, 15, docs_row_y));
    assert_eq!(state.metrics().counts.read_files, 2);

    press_key(&mut state, KeyCode::Char('i'));
    draw_screen_with_layout(&mut terminal, &mut state, &mut layout);
    assert_eq!(
        state.mode(),
        UiMode::Inspector {
            tab: InspectorTab::Unread,
            scroll: 0,
        }
    );

    let disc_rect = layout.inspector_tab_rect(InspectorTab::Discarded);
    assert!(click_left(
        &mut state,
        &layout,
        disc_rect.x + 1,
        disc_rect.y
    ));
    draw_screen_with_layout(&mut terminal, &mut state, &mut layout);
    assert_eq!(
        state.mode(),
        UiMode::Inspector {
            tab: InspectorTab::Discarded,
            scroll: 0,
        }
    );

    let tabs_bar = layout.inspector_tabs;
    assert!(!click_left(
        &mut state,
        &layout,
        tabs_bar.x + tabs_bar.width - 1,
        tabs_bar.y
    ));
    assert!(matches!(
        state.mode(),
        UiMode::Inspector {
            tab: InspectorTab::Discarded,
            ..
        }
    ));

    let unread_rect = layout.inspector_tab_rect(InspectorTab::Unread);
    assert!(click_left(
        &mut state,
        &layout,
        unread_rect.x + 1,
        unread_rect.y
    ));
    draw_screen_with_layout(&mut terminal, &mut state, &mut layout);

    let body = layout.inspector_body;
    assert!(click_left(&mut state, &layout, body.x + 2, body.y));
    assert_eq!(state.metrics().counts.read_files, 3);
}

#[test]
fn tui_mouse_inspector_tall_viewport_scroll_click_targets_exact_row() {
    let mut tree = DirectoryTree::new("tall_ws");
    for idx in 0..25 {
        tree.add_text_file(
            DirectoryTree::ROOT,
            format!("ign_{idx:02}.log"),
            TextFileData {
                byte_len: 30,
                tokens: 10,
                secret_hits: 0,
                state: TextFileState::new(NaturalSelection::Excluded(NaturalExclusion::GitIgnore)),
            },
        )
        .expect("add ignored file");
    }

    let mut state = TuiAppState::new(tree, false);
    let mut layout = TuiViewportAreas::default();
    let mut terminal = Terminal::new(TestBackend::new(100, 36)).expect("tall test terminal");

    press_key(&mut state, KeyCode::Char('i'));
    draw_screen_with_layout(&mut terminal, &mut state, &mut layout);
    press_key(&mut state, KeyCode::End);
    draw_screen_with_layout(&mut terminal, &mut state, &mut layout);

    let body = layout.inspector_body;
    assert!(usize::from(body.height) > 14);

    let expected_top_idx = match state.mode() {
        UiMode::Inspector { scroll, .. } => scroll,
        other => panic!("expected Inspector mode, got {other:?}"),
    };
    assert!(expected_top_idx > 0);

    assert!(click_left(&mut state, &layout, body.x + 2, body.y));

    let expected_name = format!("ign_{expected_top_idx:02}.log");
    let selected_names: Vec<&str> = state
        .tree()
        .nodes()
        .iter()
        .filter_map(|n| match &n.kind {
            NodeKind::TextFile(f) if f.state.is_selected() => Some(n.rel_path.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(selected_names, vec![expected_name.as_str()]);
}
