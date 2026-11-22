//! Main tree rendering, color weight tiers, folding, Win32 release guard, and narrow viewports.

use cpdir::domain::{
    DirectoryTree, NaturalExclusion, NaturalSelection, TextFileData, TextFileState,
};
use cpdir::output::render_strict_xml;
use cpdir::tui::AppExitAction;
use cpdir::tui::TuiAppState;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;
use ratatui::style::Color;

use crate::common::scan_helpers::scan_and_analyze;
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
    assert!(screen.contains("[g] Git"));
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
