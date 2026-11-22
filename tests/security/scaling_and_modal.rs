//! Linear-time `ReDoS` resistance check and TUI secret confirmation modal workflow.

use std::time::{Duration, Instant};

use cpdir::output::render_strict_xml;
use cpdir::security::SecretScanner;
use cpdir::tui::{AppExitAction, TuiAppState, UiMode};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;

use crate::common::scan_helpers::scan_and_analyze;
use crate::common::tui_harness::{draw_and_dump, move_cursor_to, press_key};
use crate::common::workspace::WorkspaceBuilder;
use crate::common::xml_validator::parse_strict_xml;

#[test]
fn hostile_near_miss_inputs_scale_linearly_without_redos_backtracking() {
    let scanner = SecretScanner::global();
    let make_hostile = |n: usize| {
        format!(
            "API_KEY = \"{}\npostgres://user:{}\nsk-proj-{}\neyJ{}\n",
            "a".repeat(n),
            "x".repeat(n),
            "-".repeat(n),
            "z".repeat(n),
        )
    };

    let small = make_hostile(20_000);
    let large = make_hostile(80_000);

    let mut min_small = Duration::MAX;
    let mut min_large = Duration::MAX;
    for _ in 0..5 {
        let t0 = Instant::now();
        assert_eq!(scanner.scan_text("hostile.txt", &small).len(), 0);
        min_small = min_small.min(t0.elapsed());

        let t1 = Instant::now();
        assert_eq!(scanner.scan_text("hostile.txt", &large).len(), 0);
        min_large = min_large.min(t1.elapsed());
    }

    let linear_ceiling = min_small.saturating_mul(12).max(Duration::from_millis(5));
    assert!(
        min_large < linear_ceiling && min_large < Duration::from_millis(250),
        "non-linear regex scaling detected: 20k={min_small:?}, 80k={min_large:?}"
    );
}

#[test]
fn end_to_end_secret_scan_tui_modal_select_all_protection_and_xml_output() {
    let ws = WorkspaceBuilder::new();
    let stripe = ["sk_live", "_51H1234567890abcdefgh"].concat();
    let aws = ["AKIA", "1234567890ABCDEF"].concat();

    ws.write_text(".gitignore", ".env\n");
    ws.write_text(".env.example", &format!("STRIPE_KEY={stripe}\n"));
    ws.write_text(
        "cfg/.env",
        &format!("AWS_KEY={aws}\nDB_PASSWORD=ProdPass99!\n"),
    );
    ws.write_text("cfg/app.toml", "port = 8080\n");
    ws.write_text("src/keys.ts", &format!("export const K = \"{stripe}\";\n"));

    let scan = scan_and_analyze(&ws);
    let mut state = TuiAppState::new(scan.tree, false);
    let mut terminal = Terminal::new(TestBackend::new(115, 26)).expect("terminal");

    let initial = draw_and_dump(&mut terminal, &mut state);
    assert!(initial.contains("! Secrets: 1"));

    press_key(&mut state, KeyCode::Enter);
    assert_eq!(
        state.mode(),
        UiMode::ConfirmSecrets {
            active_secret_files: 1
        }
    );
    let modal = draw_and_dump(&mut terminal, &mut state);
    assert!(modal.contains("- src/keys.ts (1 hit(s))"));
    assert!(!modal.contains(&stripe));

    press_key(&mut state, KeyCode::Char('n'));
    move_cursor_to(&mut state, "src/keys.ts");
    press_key(&mut state, KeyCode::Char(' '));
    let cleared = draw_and_dump(&mut terminal, &mut state);
    assert!(!cleared.contains("! Secrets:"));

    move_cursor_to(&mut state, "cfg");
    press_key(&mut state, KeyCode::Char(' '));
    press_key(&mut state, KeyCode::Char(' '));
    assert_eq!(state.metrics().counts.active_secret_files, 0);

    move_cursor_to(&mut state, "cfg/.env");
    press_key(&mut state, KeyCode::Char(' '));
    assert_eq!(state.metrics().counts.active_secret_files, 1);

    press_key(&mut state, KeyCode::Char('a'));
    assert_eq!(state.metrics().counts.active_secret_files, 1);

    press_key(&mut state, KeyCode::Enter);
    let after_a_modal = draw_and_dump(&mut terminal, &mut state);
    assert!(after_a_modal.contains("- src/keys.ts (1 hit(s))"));
    assert!(!after_a_modal.contains("cfg/.env"));

    press_key(&mut state, KeyCode::Char('y'));
    assert_eq!(state.exit_action(), Some(AppExitAction::CopyToClipboard));

    let xml = render_strict_xml(&state.into_tree(), &scan.contents, None).expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    let emitted: Vec<String> = parsed.files.into_iter().map(|f| f.path).collect();
    assert!(emitted.contains(&"src/keys.ts".to_owned()));
    assert!(emitted.contains(&".env.example".to_owned()));
    assert!(!emitted.contains(&"cfg/.env".to_owned()));
}
