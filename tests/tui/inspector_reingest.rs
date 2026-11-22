//! On-demand pruned directory re-ingestion from the `Discarded` inspector tab.

use cpdir::application::analyze_tree_contents;
use cpdir::domain::InspectorTab;
use cpdir::output::render_strict_xml;
use cpdir::scanner::{ScanConfig, rescan_pruned_dir};
use cpdir::tui::TuiAppState;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;

use crate::common::scan_helpers::scan_and_analyze;
use crate::common::tui_harness::{draw_and_dump, move_cursor_to, press_key};
use crate::common::workspace::WorkspaceBuilder;
use crate::common::xml_validator::parse_strict_xml;

#[test]
fn tui_discarded_tab_reingests_pruned_dir_collapsed_and_unselected() {
    let ws = WorkspaceBuilder::new();
    ws.write_text("src/main.rs", "fn main() {}\n");
    ws.write_text("apps/web/src/index.ts", "export const ok = true;\n");
    ws.write_text("apps/web/dist/bundle.js", "console.log('bundle');\n");
    ws.write_text("apps/web/dist/nested/style.css", "body { color: red; }\n");
    ws.populate_files("vendor", 15, "dep");

    let mut scan = scan_and_analyze(&ws);
    let mut state = TuiAppState::new(scan.tree, false);
    let mut terminal = Terminal::new(TestBackend::new(110, 24)).expect("test terminal");

    assert_eq!(state.metrics().counts.read_files, 2);
    assert_eq!(state.metrics().counts.discarded_items, 2);

    press_key(&mut state, KeyCode::Char('d'));
    assert!(draw_and_dump(&mut terminal, &mut state).contains("apps/web/dist/"));

    press_key(&mut state, KeyCode::Down);
    press_key(&mut state, KeyCode::Char(' '));
    let rel = state
        .take_pending_reingest()
        .expect("must request pruned dir re-ingest");
    assert_eq!(rel, "apps/web/dist");

    let mut sub = rescan_pruned_dir(ws.root(), &rel, &ScanConfig::default(), 2_000)
        .expect("rescan apps/web/dist");
    analyze_tree_contents(&mut sub.tree, &sub.contents).expect("analyze reingested");

    let outcome = state
        .apply_reingested_dir(&rel, sub.tree, sub.truncated)
        .expect("merge subtree");
    for (old_id, new_id) in outcome.id_map {
        if let Some(text) = sub.contents.remove(&old_id) {
            scan.contents.insert(new_id, text);
        }
    }

    let after_load = draw_and_dump(&mut terminal, &mut state);
    assert!(after_load.contains("Loaded apps/web/dist/ (collapsed & unselected)"));
    assert_eq!(state.metrics().counts.read_files, 2);
    assert_eq!(state.metrics().counts.unread_files, 2);
    assert_eq!(state.metrics().counts.discarded_items, 1);

    // Re-ingest `vendor` with tight `max_entries = 5` to verify truncation safety guard.
    press_key(&mut state, KeyCode::Char(' '));
    let vendor_rel = state.take_pending_reingest().expect("vendor request");
    assert_eq!(vendor_rel, "vendor");
    let vendor_sub = rescan_pruned_dir(ws.root(), &vendor_rel, &ScanConfig::default(), 5)
        .expect("capped rescan");
    assert!(vendor_sub.truncated);
    state.apply_reingested_dir(&vendor_rel, vendor_sub.tree, vendor_sub.truncated);
    assert!(draw_and_dump(&mut terminal, &mut state).contains("truncated at entry cap"));

    // Close modal and verify `[a]` All/None never pulls unselected pruned files in.
    press_key(&mut state, KeyCode::Esc);
    press_key(&mut state, KeyCode::Char('a'));
    assert_eq!(state.metrics().counts.read_files, 0);
    press_key(&mut state, KeyCode::Char('a'));
    assert_eq!(state.metrics().counts.read_files, 2);

    assert!(
        state
            .visible_rows()
            .iter()
            .any(|r| state.tree().nodes()[r.node_id.index()].rel_path == "apps/web/dist")
    );
    assert!(
        !state
            .visible_rows()
            .iter()
            .any(|r| state.tree().nodes()[r.node_id.index()].rel_path == "apps/web/dist/bundle.js")
    );

    move_cursor_to(&mut state, "apps/web/dist");
    press_key(&mut state, KeyCode::Tab);
    move_cursor_to(&mut state, "apps/web/dist/bundle.js");
    press_key(&mut state, KeyCode::Char(' '));

    let xml = render_strict_xml(state.tree(), &scan.contents, None).expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    let emitted: Vec<String> = parsed.files.into_iter().map(|f| f.path).collect();
    assert!(emitted.contains(&"apps/web/dist/bundle.js".to_owned()));
    assert!(!emitted.contains(&"apps/web/dist/nested/style.css".to_owned()));
}

#[test]
fn tui_inspector_sanitizes_bidi_and_control_chars_in_status_note() {
    let ws = WorkspaceBuilder::new();
    ws.write_text("src/main.rs", "fn main() {}\n");

    let scan = scan_and_analyze(&ws);
    let mut state = TuiAppState::new(scan.tree, false);
    let mut terminal = Terminal::new(TestBackend::new(100, 22)).expect("test terminal");

    state.open_inspector_tab(InspectorTab::Discarded);
    state.set_inspector_status("Failed \u{202E}bad\x1B/");
    let screen = draw_and_dump(&mut terminal, &mut state);

    assert!(screen.contains("Failed \u{FFFD}bad\u{FFFD}/"));
    assert!(!screen.contains('\u{202E}'));
    assert!(!screen.contains('\x1B'));
}
