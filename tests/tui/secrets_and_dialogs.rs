//! Purple secret cues, Git token accounting, loading view, help modal, and copy guards.

use cpdir::domain::{
    DirectoryTree, NaturalExclusion, NaturalSelection, SecretFinding, SecretRuleKind, TextFileData,
    TextFileState,
};
use cpdir::tui::{
    AppExitAction, LoadingPhase, LoadingProgress, TuiAppState, UiMode, render_loading_view,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;
use ratatui::style::Color;

use crate::common::tui_harness::{
    buffer_to_string, draw_and_dump, find_substring_fg, move_cursor_to, press_key,
};

#[test]
fn tui_secret_warnings_inspector_tab_git_tokens_and_exclude_action() {
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
    let env_id = tree
        .add_text_file(
            DirectoryTree::ROOT,
            ".env",
            TextFileData {
                byte_len: 120,
                tokens: 50,
                secret_hits: 1,
                state: TextFileState::new(NaturalSelection::Excluded(NaturalExclusion::GitIgnore)),
            },
        )
        .expect(".env");
    tree.set_secret_findings(
        env_id,
        vec![SecretFinding {
            line: 4,
            rule: SecretRuleKind::AwsAccessKey,
        }],
    );

    let mut state = TuiAppState::new(tree.clone(), true);
    state.set_git_context_tokens(1_400);
    let mut terminal = Terminal::new(TestBackend::new(115, 24)).expect("test terminal");

    let initial = draw_and_dump(&mut terminal, &mut state);
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
    let after_git_off = draw_and_dump(&mut terminal, &mut state);
    assert!(!state.include_git_context());
    assert!(after_git_off.contains("Tokens: 400"));
    assert!(after_git_off.contains("Git: OFF"));

    press_key(&mut state, KeyCode::Char('s'));
    let sec_tab = draw_and_dump(&mut terminal, &mut state);
    assert!(sec_tab.contains("Secrets (1)"));
    assert!(sec_tab.contains("[AwsAccessKey] .env:4"));

    press_key(&mut state, KeyCode::Char(' '));
    assert_eq!(state.metrics().counts.active_secret_files, 1);
    press_key(&mut state, KeyCode::Esc);

    press_key(&mut state, KeyCode::Enter);
    assert_eq!(
        state.mode(),
        UiMode::ConfirmSecrets {
            active_secret_files: 1
        }
    );
    let modal = draw_and_dump(&mut terminal, &mut state);
    assert!(modal.contains("- .env (1 hit(s)) - line 4 [AwsAccessKey]"));
    assert!(modal.contains("[x] Exclude flagged"));

    // Pressing 's' inside ConfirmSecrets must NOT silently confirm copy.
    press_key(&mut state, KeyCode::Char('s'));
    assert_eq!(state.exit_action(), None);
    assert_eq!(
        state.mode(),
        UiMode::ConfirmSecrets {
            active_secret_files: 1
        }
    );

    press_key(&mut state, KeyCode::Char('x'));
    assert_eq!(state.mode(), UiMode::Tree);
    assert_eq!(state.metrics().counts.active_secret_files, 0);

    let mut guarded = TuiAppState::new(tree, true);
    move_cursor_to(&mut guarded, ".env");
    press_key(&mut guarded, KeyCode::Char(' '));
    let _ = draw_and_dump(&mut terminal, &mut guarded);
    assert_eq!(
        find_substring_fg(&terminal, ".env"),
        Some(Color::Rgb(205, 130, 255))
    );
    press_key(&mut guarded, KeyCode::Enter);
    press_key(&mut guarded, KeyCode::Char('y'));
    assert_eq!(guarded.exit_action(), Some(AppExitAction::CopyToClipboard));
}

#[test]
fn tui_help_modal_unselect_loading_view_and_empty_copy_guard() {
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
    let help = draw_and_dump(&mut terminal, &mut state);
    assert!(help.contains("Help - Press Esc or ? to close"));
    assert!(help.contains("a / u"));
    assert!(help.contains("Secret[!]"));

    press_key(&mut state, KeyCode::Esc);
    assert_eq!(state.mode(), UiMode::Tree);

    press_key(&mut state, KeyCode::Char('c'));
    assert_eq!(state.visible_rows().len(), 2);
    press_key(&mut state, KeyCode::Char('e'));
    assert_eq!(state.visible_rows().len(), 3);
    press_key(&mut state, KeyCode::End);
    assert_eq!(state.cursor(), 2);
    press_key(&mut state, KeyCode::Home);
    assert_eq!(state.cursor(), 0);

    press_key(&mut state, KeyCode::Char('u'));
    assert_eq!(state.metrics().counts.read_files, 0);
    press_key(&mut state, KeyCode::Enter);
    assert_eq!(state.mode(), UiMode::ConfirmEmptyCopy);
    assert!(draw_and_dump(&mut terminal, &mut state).contains("0 files are selected"));

    press_key(&mut state, KeyCode::Char('n'));
    assert_eq!(state.mode(), UiMode::Tree);
    assert_eq!(state.exit_action(), None);

    press_key(&mut state, KeyCode::Enter);
    press_key(&mut state, KeyCode::Char('y'));
    assert_eq!(state.exit_action(), Some(AppExitAction::CopyToClipboard));

    let mut prog = LoadingProgress::new("vulkan_engine");
    prog.phase = LoadingPhase::Analyzing;
    prog.dirs_scanned = 120;
    prog.pruned_dirs = 8;
    prog.candidate_files = 450;
    prog.analyzed_files = 225;
    prog.discarded_files = 90;
    prog.selected_tokens = 38_400;
    terminal
        .draw(|frame| render_loading_view(frame, frame.area(), &prog))
        .expect("draw loading view");
    let loading_screen = buffer_to_string(&terminal);
    assert!(loading_screen.contains("vulkan_engine"));
    assert!(loading_screen.contains("225/450 analyzed"));
    assert!(loading_screen.contains("38.4k selected"));
    assert!(loading_screen.contains("Cancel startup"));
}
