//! Negative-corpus tests ensuring templates, lockfiles, certificates, and UI labels stay clean.

use cpdir::security::SecretScanner;

use crate::common::scan_helpers::scan_and_analyze;
use crate::common::workspace::WorkspaceBuilder;

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
    assert_eq!(scanner.scan_text("src/locales/en.ts", clean_ui), vec![]);

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

    let scan = scan_and_analyze(&ws);
    let counts = scan.tree.compute_metrics().counts;
    assert_eq!(counts.total_secret_files, 0);
    assert_eq!(counts.active_secret_files, 0);
}
