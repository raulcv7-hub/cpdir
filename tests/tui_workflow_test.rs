//! End-to-end TUI workflows driven through `ratatui::backend::TestBackend`.

mod common;

use common::workspace::{WorkspaceBuilder, mid_stream_corrupt_utf8_bytes, sample_png_bytes};
use common::xml_validator::parse_strict_xml;
use cpdir::config::defaults::DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD;
use cpdir::domain::{
    DirectoryTree, NaturalExclusion, NaturalSelection, TextFileData, TextFileState,
};
use cpdir::output::render_strict_xml;
use cpdir::scanner::{ScanConfig, ScanResult, scan_directory};
use cpdir::tokens::{TokenCounter, populate_token_counts};
use cpdir::tui::{
    AppExitAction, TuiAppState, UiMode, handle_key_event, render_active_modal, render_tree_view,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::Color;

fn draw_screen(terminal: &mut Terminal<TestBackend>, state: &mut TuiAppState) {
    terminal
        .draw(|frame| {
            let area = frame.area();
            render_tree_view(frame, area, state);
            render_active_modal(frame, area, state);
        })
        .expect("draw test backend");
}

fn buffer_to_string(terminal: &Terminal<TestBackend>) -> String {
    let buf = terminal.backend().buffer();
    let mut lines = Vec::with_capacity(usize::from(buf.area.height));
    for y in buf.area.y..(buf.area.y + buf.area.height) {
        let mut line = String::new();
        for x in buf.area.x..(buf.area.x + buf.area.width) {
            if let Some(cell) = buf.cell((x, y)) {
                line.push_str(cell.symbol());
            }
        }
        lines.push(line.trim_end().to_owned());
    }
    lines.join("\n")
}

fn find_substring_fg(terminal: &Terminal<TestBackend>, needle: &str) -> Option<Color> {
    let buf = terminal.backend().buffer();
    for y in buf.area.y..(buf.area.y + buf.area.height) {
        let mut line = String::new();
        for x in buf.area.x..(buf.area.x + buf.area.width) {
            if let Some(cell) = buf.cell((x, y)) {
                line.push_str(cell.symbol());
            }
        }
        if let Some(byte_idx) = line.find(needle) {
            let char_col = u16::try_from(line[..byte_idx].chars().count()).ok()?;
            return buf.cell((buf.area.x + char_col, y)).map(|c| c.fg);
        }
    }
    None
}

fn press_key(state: &mut TuiAppState, code: KeyCode) {
    handle_key_event(state, KeyEvent::new(code, KeyModifiers::NONE));
}

fn release_key(state: &mut TuiAppState, code: KeyCode) {
    let mut ev = KeyEvent::new(code, KeyModifiers::NONE);
    ev.kind = KeyEventKind::Release;
    handle_key_event(state, ev);
}

fn move_cursor_to(state: &mut TuiAppState, target_rel: &str) {
    let max_steps = state.visible_rows().len();
    for _ in 0..max_steps {
        let row = state.visible_rows()[state.cursor()];
        if state.tree().nodes()[row.node_id.index()].rel_path == target_rel {
            return;
        }
        press_key(state, KeyCode::Down);
    }
    panic!("could not navigate cursor to {target_rel}");
}

fn scan_and_tokenize(ws: &WorkspaceBuilder) -> ScanResult {
    let mut scan = scan_directory(ws.root(), &ScanConfig::default()).expect("scan");
    let counter = TokenCounter::o200k().expect("o200k");
    populate_token_counts(
        &mut scan.tree,
        &scan.contents,
        DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD,
        counter,
    );
    scan
}

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

    let scan = scan_and_tokenize(&ws);
    let mut state = TuiAppState::new(scan.tree, false);
    let mut terminal = Terminal::new(TestBackend::new(115, 28)).expect("test terminal");

    draw_screen(&mut terminal, &mut state);
    let initial = buffer_to_string(&terminal);
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

    // Win32 Release event must not toggle root selection.
    release_key(&mut state, KeyCode::Char(' '));
    assert_eq!(state.metrics().counts.read_files, 3);

    // Toggle entire `src` folder off -> dimmed green hue and reduced count; then fold it.
    move_cursor_to(&mut state, "src");
    press_key(&mut state, KeyCode::Char(' '));
    draw_screen(&mut terminal, &mut state);
    let after_toggle = buffer_to_string(&terminal);
    assert!(after_toggle.contains("Read: 1"));
    assert_eq!(
        find_substring_fg(&terminal, "main.rs"),
        Some(Color::Rgb(42, 102, 60))
    );

    press_key(&mut state, KeyCode::Tab);
    draw_screen(&mut terminal, &mut state);
    assert!(!buffer_to_string(&terminal).contains("helper.rs"));

    press_key(&mut state, KeyCode::Enter);
    assert_eq!(state.exit_action(), Some(AppExitAction::CopyToClipboard));

    let final_tree = state.into_tree();
    let xml = render_strict_xml(&final_tree, &scan.contents, None).expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    let dir_tree = parsed.directory_structure.expect("directory_structure");

    assert!(dir_tree.contains("helper.rs"));
    assert!(dir_tree.contains("empty_dir/"));
    assert!(dir_tree.contains("secret.log"));
    let emitted: Vec<String> = parsed.files.into_iter().map(|f| f.path).collect();
    assert_eq!(emitted, vec![".gitignore"]);
}

#[test]
fn tui_inspector_modals_select_all_none_and_bidi_sanitization() {
    let ws = WorkspaceBuilder::new();
    ws.write_text(".gitignore", "*.log\n");
    ws.write_text("src/app.rs", "pub fn run() {}\n");
    ws.write_text("ignored.log", "ignored log\n");
    ws.write_bytes("assets/logo.png", &sample_png_bytes());
    ws.write_bytes("broken.rs", &mid_stream_corrupt_utf8_bytes());
    ws.populate_files("node_modules/dep", 5, "dep");

    let scan = scan_and_tokenize(&ws);
    let mut state = TuiAppState::new(scan.tree, false);
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).expect("test terminal");

    press_key(&mut state, KeyCode::Char('d'));
    draw_screen(&mut terminal, &mut state);
    let disc_view = buffer_to_string(&terminal);
    assert!(disc_view.contains("Discarded (3)"));
    assert!(disc_view.contains("[PRUNED]"));
    assert!(disc_view.contains("[BIN]"));
    assert!(disc_view.contains("[UTF8]"));

    press_key(&mut state, KeyCode::Char('n'));
    draw_screen(&mut terminal, &mut state);
    let unread_view = buffer_to_string(&terminal);
    assert!(unread_view.contains("Unread (1)"));
    assert!(unread_view.contains("[.gitignore]"));

    press_key(&mut state, KeyCode::Esc);
    assert_eq!(state.mode(), UiMode::Tree);

    // Single `a` key toggles between None and All, and `u` remains a direct alias for None.
    press_key(&mut state, KeyCode::Char('a'));
    assert_eq!(state.metrics().counts.read_files, 0);
    press_key(&mut state, KeyCode::Char('a'));
    assert_eq!(state.metrics().counts.read_files, 2);
    press_key(&mut state, KeyCode::Char('u'));
    assert_eq!(state.metrics().counts.read_files, 0);
    press_key(&mut state, KeyCode::Char('a'));
    assert_eq!(state.metrics().counts.read_files, 2);

    // Sanitize hostile Bidi/ANSI filename characters before writing cells to terminal.
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
    draw_screen(&mut terminal, &mut bidi_state);
    let bidi_screen = buffer_to_string(&terminal);
    assert!(bidi_screen.contains("evil\u{FFFD}gpj.rs\u{FFFD}"));
    assert!(!bidi_screen.contains('\u{202E}'));
    assert!(!bidi_screen.contains('\x1B'));
}

#[test]
fn tui_secret_warnings_purple_tint_git_tokens_and_confirmation_modal() {
    let mut tree = DirectoryTree::new("sec_repo");
    tree.add_text_file(
        DirectoryTree::ROOT,
        "main.rs",
        TextFileData {
            byte_len: 800,
            tokens: 400,
            secret_hits: 0,
            state: TextFileState::new(NaturalSelection::Included),
        },
    )
    .expect("main.rs");
    tree.add_text_file(
        DirectoryTree::ROOT,
        ".env",
        TextFileData {
            byte_len: 120,
            tokens: 50,
            secret_hits: 2,
            state: TextFileState::new(NaturalSelection::Excluded(NaturalExclusion::GitIgnore)),
        },
    )
    .expect(".env");

    let mut state = TuiAppState::new(tree.clone(), true);
    state.set_git_context_tokens(1_400);
    let mut terminal = Terminal::new(TestBackend::new(115, 24)).expect("test terminal");

    draw_screen(&mut terminal, &mut state);
    let initial = buffer_to_string(&terminal);
    assert!(initial.contains("Tokens: 1.8k"));
    assert!(initial.contains("Git: ON (+1.4k)"));
    assert!(initial.contains("[g] Git"));
    assert!(initial.contains("[!]"));
    assert!(!initial.contains("! Secrets:"));
    assert_eq!(
        find_substring_fg(&terminal, ".env"),
        Some(Color::Rgb(115, 72, 145))
    );

    press_key(&mut state, KeyCode::Char('g'));
    draw_screen(&mut terminal, &mut state);
    assert!(!state.include_git_context());
    let after_git_off = buffer_to_string(&terminal);
    assert!(after_git_off.contains("Tokens: 400"));
    assert!(after_git_off.contains("Git: OFF"));

    // Unselected secret file does not block Enter.
    press_key(&mut state, KeyCode::Enter);
    assert_eq!(state.exit_action(), Some(AppExitAction::CopyToClipboard));

    // Selecting `.env` turns it bright purple, activates header warning, and opens ConfirmSecrets.
    let mut guarded = TuiAppState::new(tree, true);
    move_cursor_to(&mut guarded, ".env");
    press_key(&mut guarded, KeyCode::Char(' '));
    draw_screen(&mut terminal, &mut guarded);
    assert!(buffer_to_string(&terminal).contains("! Secrets: 1"));
    assert_eq!(
        find_substring_fg(&terminal, ".env"),
        Some(Color::Rgb(205, 130, 255))
    );

    press_key(&mut guarded, KeyCode::Enter);
    assert_eq!(
        guarded.mode(),
        UiMode::ConfirmSecrets {
            active_secret_files: 1
        }
    );
    draw_screen(&mut terminal, &mut guarded);
    let modal = buffer_to_string(&terminal);
    assert!(modal.contains("Potential Secrets Selected"));
    assert!(modal.contains("1 selected file(s) contain potential secrets"));

    press_key(&mut guarded, KeyCode::Char('n'));
    assert_eq!(guarded.mode(), UiMode::Tree);
    assert_eq!(guarded.exit_action(), None);

    // Pressing `a` while custom overrides exist restores safe defaults without keeping `.env`.
    press_key(&mut guarded, KeyCode::Char('a'));
    assert_eq!(guarded.metrics().counts.read_files, 1);
    assert_eq!(guarded.metrics().counts.active_secret_files, 0);

    // Re-enable `.env` and confirm copy anyway.
    press_key(&mut guarded, KeyCode::Char(' '));
    press_key(&mut guarded, KeyCode::Enter);
    press_key(&mut guarded, KeyCode::Char('y'));
    assert_eq!(guarded.exit_action(), Some(AppExitAction::CopyToClipboard));
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

    draw_screen(&mut terminal, &mut state);
    let screen = buffer_to_string(&terminal);

    // Header alerts, git status, and right-pinned file badges stay visible at 76 columns.
    assert!(screen.contains("! Secrets: 1"));
    assert!(screen.contains("Git: ON"));
    assert!(screen.contains("[g] Git"));
    assert!(screen.contains("[Esc] Quit"));
    assert!(screen.contains("… (420) [.gitignore] [!]"));
    assert!(screen.contains("├─") || screen.contains("└─"));
    assert!(screen.contains('█'));

    // Scrolloff keeps upcoming rows visible when moving near bottom of viewport.
    for _ in 0..10 {
        press_key(&mut state, KeyCode::Down);
        draw_screen(&mut terminal, &mut state);
    }
    assert!(state.scroll_offset() > 0);
    assert!(state.cursor() < state.visible_rows().len());
}

#[test]
fn tui_help_modal_collapse_expand_all_and_empty_copy_guard() {
    let mut tree = DirectoryTree::new("workspace");
    let sub = tree.add_dir(DirectoryTree::ROOT, "sub", true).expect("sub");
    tree.add_text_file(
        sub,
        "lib.rs",
        TextFileData {
            byte_len: 100,
            tokens: 40,
            secret_hits: 0,
            state: TextFileState::new(NaturalSelection::Included),
        },
    )
    .expect("lib.rs");

    let mut state = TuiAppState::new(tree, false);
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).expect("test terminal");

    press_key(&mut state, KeyCode::Char('?'));
    assert_eq!(state.mode(), UiMode::Help);
    draw_screen(&mut terminal, &mut state);
    let help = buffer_to_string(&terminal);
    assert!(help.contains("Help - Press Esc or ? to close"));
    assert!(help.contains("Secret[!]"));

    press_key(&mut state, KeyCode::Esc);
    assert_eq!(state.mode(), UiMode::Tree);

    // Collapse all folds `sub`, expand all restores `lib.rs`, Home/End jump to bounds.
    press_key(&mut state, KeyCode::Char('c'));
    assert_eq!(state.visible_rows().len(), 2);
    press_key(&mut state, KeyCode::Char('e'));
    assert_eq!(state.visible_rows().len(), 3);
    press_key(&mut state, KeyCode::End);
    assert_eq!(state.cursor(), 2);
    press_key(&mut state, KeyCode::Home);
    assert_eq!(state.cursor(), 0);

    // Deselecting all triggers empty-selection confirmation on Enter.
    press_key(&mut state, KeyCode::Char('a'));
    assert_eq!(state.metrics().counts.read_files, 0);
    press_key(&mut state, KeyCode::Enter);
    assert_eq!(state.mode(), UiMode::ConfirmEmptyCopy);
    draw_screen(&mut terminal, &mut state);
    assert!(buffer_to_string(&terminal).contains("0 files are selected"));

    press_key(&mut state, KeyCode::Char('n'));
    assert_eq!(state.mode(), UiMode::Tree);
    assert_eq!(state.exit_action(), None);

    press_key(&mut state, KeyCode::Enter);
    press_key(&mut state, KeyCode::Char('y'));
    assert_eq!(state.exit_action(), Some(AppExitAction::CopyToClipboard));
}
