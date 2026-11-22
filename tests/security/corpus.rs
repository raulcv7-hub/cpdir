//! Adversarial credential corpus and line/rule retention tests for `SecretScanner`.

use cpdir::domain::NodeKind;
use cpdir::scanner::{ScanConfig, scan_directory};
use cpdir::security::{SecretFinding, SecretRuleKind, SecretScanner, populate_secret_hits};

use crate::common::workspace::WorkspaceBuilder;

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
fn populate_secret_hits_preserves_line_numbers_and_rule_kinds_on_tree() {
    let ws = WorkspaceBuilder::new();
    let aws_key = ["AKIA", "1234567890ABCDEF"].concat();
    let prod_env = format!(
        "# production config\nAPP_ENV=production\nAWS_KEY={aws_key}\n\n# db\n\nDB_PASSWORD=Sup3r_Real_Prod_Pass!\n"
    );
    let example_env = format!("AWS_KEY={aws_key}\n");

    ws.write_text(".env.production", &prod_env);
    ws.write_text(".env.local.example", &example_env);

    let mut scan = scan_directory(ws.root(), &ScanConfig::default()).expect("scan");
    populate_secret_hits(&mut scan.tree, &scan.contents, SecretScanner::global());

    let env_id = scan
        .tree
        .visible_rows()
        .into_iter()
        .find(|r| scan.tree.nodes()[r.node_id.index()].rel_path == ".env.production")
        .map(|r| r.node_id)
        .expect("visible .env.production row");

    let NodeKind::TextFile(env_file) = &scan.tree.nodes()[env_id.index()].kind else {
        panic!("expected text node");
    };
    assert_eq!(env_file.secret_hits, 2);
    assert_eq!(
        scan.tree.secret_findings(env_id),
        &[
            SecretFinding {
                line: 3,
                rule: SecretRuleKind::AwsAccessKey,
            },
            SecretFinding {
                line: 7,
                rule: SecretRuleKind::HardcodedSecret,
            },
        ]
    );

    let ex_id = scan
        .tree
        .visible_rows()
        .into_iter()
        .find(|r| scan.tree.nodes()[r.node_id.index()].rel_path == ".env.local.example")
        .map(|r| r.node_id)
        .expect("visible .env.local.example row");
    assert_eq!(scan.tree.secret_findings(ex_id), []);
}
