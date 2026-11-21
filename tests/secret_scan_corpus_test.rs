//! Clean real-world negative corpus and detailed finding retention tests for `SecretScanner`.

mod common;

use common::workspace::WorkspaceBuilder;
use cpdir::domain::NodeKind;
use cpdir::scanner::{ScanConfig, scan_directory};
use cpdir::security::{SecretFinding, SecretRuleKind, SecretScanner, populate_secret_hits};

#[test]
fn clean_code_i18n_templates_and_certificates_produce_zero_false_positives() {
    let scanner = SecretScanner::global();

    let clean_ui = concat!(
        "const labels = {\n",
        "  password: \"Password\",\n",
        "  api_key: \"X-API-KEY\",\n",
        "  access_token: \"Access Token\",\n",
        "  secret_key: \"Enter your secret\",\n",
        "  db_password: \"Invalid password\",\n",
        "  auth_token: \"Bearer {{API_TOKEN}}\",\n",
        "  client_secret: \"prefix-${CLIENT_SECRET}\",\n",
        "  private_key: \"<insert-key-here>\",\n",
        "};\n",
    );
    assert_eq!(
        scanner.scan_text("src/locales/en.ts", clean_ui),
        vec![],
        "i18n/UI label strings must not trigger false positives"
    );

    let clean_cert = concat!(
        "-----BEGIN CERTIFICATE-----\n",
        "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA\n",
        "-----END CERTIFICATE-----\n",
        "// sha256: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n",
    );
    assert_eq!(scanner.scan_text("certs/ca.pem", clean_cert), vec![]);

    let fake_aws = ["AKIA", "IOSFODNN7EXAMPLE"].concat();
    let template_body = format!("AWS_ACCESS_KEY_ID={fake_aws}\n");
    for exempt_path in [
        ".env.local.example",
        "deploy/.env.production.sample",
        "config/settings.example.yaml",
        "infra/secrets.template.json",
    ] {
        assert_eq!(
            scanner.scan_text(exempt_path, &template_body),
            vec![],
            "template {exempt_path} must remain exempt"
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
