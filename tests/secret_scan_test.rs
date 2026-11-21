//! Adversarial secret corpus, allowlist, linear-time scaling, and TUI modal e2e tests.

mod common;

use std::time::{Duration, Instant};

use common::workspace::WorkspaceBuilder;
use common::xml_validator::parse_strict_xml;
use cpdir::config::defaults::DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD;
use cpdir::output::render_strict_xml;
use cpdir::scanner::{ScanConfig, ScanResult, scan_directory};
use cpdir::security::{SecretRuleKind, SecretScanner, populate_secret_hits};
use cpdir::tokens::{TokenCounter, populate_token_counts};
use cpdir::tui::{
    AppExitAction, TuiAppState, UiMode, handle_key_event, render_active_modal, render_tree_view,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn draw_and_dump(terminal: &mut Terminal<TestBackend>, state: &mut TuiAppState) -> String {
    terminal
        .draw(|frame| {
            let area = frame.area();
            render_tree_view(frame, area, state);
            render_active_modal(frame, area, state);
        })
        .expect("draw test backend");

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

fn press(state: &mut TuiAppState, code: KeyCode) {
    handle_key_event(state, KeyEvent::new(code, KeyModifiers::NONE));
}

fn move_to(state: &mut TuiAppState, target_rel: &str) {
    let Some(target_idx) = state
        .visible_rows()
        .iter()
        .position(|r| state.tree().nodes()[r.node_id.index()].rel_path == target_rel)
    else {
        panic!("missing row {target_rel}");
    };
    while state.cursor() != target_idx {
        if state.cursor() < target_idx {
            press(state, KeyCode::Down);
        } else {
            press(state, KeyCode::Up);
        }
    }
}

fn scan_full(ws: &WorkspaceBuilder) -> ScanResult {
    let mut scan = scan_directory(ws.root(), &ScanConfig::default()).expect("scan");
    let counter = TokenCounter::o200k().expect("o200k");
    populate_token_counts(
        &mut scan.tree,
        &scan.contents,
        DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD,
        counter,
    );
    populate_secret_hits(&mut scan.tree, &scan.contents, SecretScanner::global());
    scan
}

#[test]
fn detects_adversarial_secret_corpus_across_all_supported_rule_categories() {
    let scanner = SecretScanner::global();

    // Split fake test tokens via `.concat()` so GitHub Push Protection never blocks pushes.
    let pem = [
        "-----BEGIN OPENSSH ",
        "PRIVATE KEY-----\nAAAA\n-----END OPENSSH ",
        "PRIVATE KEY-----\n",
    ]
    .concat();
    let aws = format!(
        "const AWS: &str = \"{}\";\n",
        ["AKIA", "1234567890ABCDEF"].concat()
    );
    let gh = format!(
        "const GH: &str = \"{}\";\nconst PAT: &str = \"{}\";\n",
        ["ghp", "_1234567890abcdefghijklmnopqr"].concat(),
        ["github_pat", "_1234567890abcdefghijklmnopqr"].concat()
    );
    let google = format!(
        "const G: &str = \"{}\";\n",
        ["AIza", "SyA1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q"].concat()
    );
    let slack = format!(
        "SLACK = \"{}\"\n",
        ["xoxb", "-1234567890-1234567890-abcdef"].concat()
    );
    let stripe = format!(
        "STRIPE = \"{}\"\n",
        ["sk_live", "_51H1234567890abcdefgh"].concat()
    );
    let ai = format!(
        "OPENAI = \"{}\"\nCLAUDE = \"{}\"\n",
        ["sk-proj", "-AbCdEf1234567890AbCdEf1234"].concat(),
        ["sk-ant", "-AbCdEf1234567890AbCdEf1234"].concat()
    );
    let db_url = format!(
        "url = \"{}\"\n",
        ["postgres://produser", ":RealSecret99!@db.internal:5432/app"].concat()
    );
    let jwt = format!(
        "{{\"jwt\":\"{}\"}}\n",
        [
            "eyJhbGciOiJIUzI1NiJ9",
            ".eyJzdWIiOiIxMjM0NTY3ODkwIn0",
            ".SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"
        ]
        .concat()
    );
    let dotenv = "DB_PASSWORD=MyRealProdPass99!\napi_key = \"hardcoded_prod_secret_88\"\n";

    let cases: &[(&str, &str, SecretRuleKind, usize)] = &[
        ("keys/id_rsa", &pem, SecretRuleKind::PrivateKey, 1),
        ("src/aws.rs", &aws, SecretRuleKind::AwsAccessKey, 1),
        ("src/gh.ts", &gh, SecretRuleKind::GitHubToken, 2),
        ("src/google.js", &google, SecretRuleKind::GoogleApiKey, 1),
        ("src/slack.py", &slack, SecretRuleKind::SlackToken, 1),
        ("src/stripe.ts", &stripe, SecretRuleKind::StripeLiveKey, 1),
        ("src/ai.rs", &ai, SecretRuleKind::AiProviderKey, 2),
        ("config/db.toml", &db_url, SecretRuleKind::CredentialUrl, 1),
        ("auth.json", &jwt, SecretRuleKind::JwtToken, 1),
        (".env", dotenv, SecretRuleKind::HardcodedSecret, 2),
    ];

    let ws = WorkspaceBuilder::new();
    for &(rel, body, expected_rule, expected_len) in cases {
        let disk_path = ws.write_text(rel, body);
        let disk_body = std::fs::read_to_string(disk_path).expect("read written fixture");
        let hits = scanner.scan_text(rel, &disk_body);
        assert_eq!(hits.len(), expected_len, "unexpected hit count for {rel}");
        assert!(
            hits.iter().all(|h| h.rule == expected_rule),
            "unexpected rule match for {rel}: {hits:?}"
        );
    }
}

#[test]
fn exempts_env_examples_lockfiles_placeholders_and_normal_code_expressions() {
    let ws = WorkspaceBuilder::new();
    let real_aws = ["AKIA", "1234567890ABCDEF"].concat();
    let example_body = format!("AWS_KEY={real_aws}\nDB_PASSWORD=RealProdPass99!\n");

    ws.write_text(".env.example", &example_body);
    ws.write_text("sub/.env.sample", &example_body);
    ws.write_text("sub/.env.template", &example_body);
    ws.write_text("Cargo.lock", &format!("checksum = \"{real_aws}\"\n"));
    ws.write_text(
        "src/config.rs",
        concat!(
            "const MY_PASSWORD: &str = get_user_password();\n",
            "const DEFAULT_API_KEY: &str = process_env_key();\n",
            "const API_KEY: &str = \"your-api-key-here\";\n",
            "const TOKEN: &str = \"${GITHUB_TOKEN}\";\n",
            "const PASS: &str = \"changeme_12345\";\n",
            "const REPEATED: &str = \"aaaaaaaaaaaa\";\n",
            "const DB_URL: &str = \"postgres://user:placeholder_pass@localhost:5432/db\";\n",
        ),
    );

    let scan = scan_full(&ws);
    let counts = scan.tree.compute_metrics().counts;
    assert_eq!(counts.total_secret_files, 0);
    assert_eq!(counts.active_secret_files, 0);
}

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

    let scan = scan_full(&ws);
    let mut state = TuiAppState::new(scan.tree, false);
    let mut terminal = Terminal::new(TestBackend::new(115, 26)).expect("terminal");

    let initial = draw_and_dump(&mut terminal, &mut state);
    assert!(initial.contains("! Secrets: 1"));

    // Enter opens ConfirmSecrets listing `src/keys.ts` with exact hit count.
    press(&mut state, KeyCode::Enter);
    assert_eq!(
        state.mode(),
        UiMode::ConfirmSecrets {
            active_secret_files: 1
        }
    );
    let modal = draw_and_dump(&mut terminal, &mut state);
    assert!(modal.contains("- src/keys.ts (1 hit(s))"));
    assert!(!modal.contains(&stripe));

    // Cancel modal, unselect `src/keys.ts`, verify header warning disappears.
    press(&mut state, KeyCode::Char('n'));
    move_to(&mut state, "src/keys.ts");
    press(&mut state, KeyCode::Char(' '));
    let cleared = draw_and_dump(&mut terminal, &mut state);
    assert!(!cleared.contains("! Secrets:"));

    // Mixed folder toggle OFF then ON keeps `cfg/.env` protected.
    move_to(&mut state, "cfg");
    press(&mut state, KeyCode::Char(' '));
    press(&mut state, KeyCode::Char(' '));
    assert_eq!(state.metrics().counts.active_secret_files, 0);

    // Manually select `cfg/.env`, hit `a` (Select All) -> `.env` resets back to unselected,
    // while non-ignored `src/keys.ts` is restored and caught by ConfirmSecrets modal.
    move_to(&mut state, "cfg/.env");
    press(&mut state, KeyCode::Char(' '));
    assert_eq!(state.metrics().counts.active_secret_files, 1);

    press(&mut state, KeyCode::Char('a'));
    assert_eq!(state.metrics().counts.active_secret_files, 1);

    press(&mut state, KeyCode::Enter);
    let after_a_modal = draw_and_dump(&mut terminal, &mut state);
    assert!(after_a_modal.contains("- src/keys.ts (1 hit(s))"));
    assert!(!after_a_modal.contains("cfg/.env"));

    press(&mut state, KeyCode::Char('y'));
    assert_eq!(state.exit_action(), Some(AppExitAction::CopyToClipboard));

    let xml = render_strict_xml(&state.into_tree(), &scan.contents, None).expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    let emitted: Vec<String> = parsed.files.into_iter().map(|f| f.path).collect();
    assert!(emitted.contains(&"src/keys.ts".to_owned()));
    assert!(emitted.contains(&".env.example".to_owned()));
    assert!(!emitted.contains(&"cfg/.env".to_owned()));
}
