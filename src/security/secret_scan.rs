//! Linear-time regex secret scanner with template allowlisting and placeholder filtering.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::sync::OnceLock;

use rayon::prelude::*;
use regex::{Regex, RegexSet};

use crate::config::defaults::is_secret_scan_exempt_path;
use crate::domain::{DirectoryTree, NodeId, NodeKind};

pub use crate::domain::{SecretFinding, SecretRuleKind};

const PARALLEL_SCAN_MIN_FILES: usize = 8;

#[derive(Debug)]
struct CompiledRule {
    kind: SecretRuleKind,
    regex: Regex,
    check_capture_placeholder: bool,
}

/// Compiled thread-safe regex secret scanner initialized once per process.
#[derive(Debug)]
pub struct SecretScanner {
    fast_set: RegexSet,
    rules: Vec<CompiledRule>,
}

static GLOBAL_SCANNER: OnceLock<SecretScanner> = OnceLock::new();

impl SecretScanner {
    /// Returns a shared reference to the compiled global [`SecretScanner`].
    #[must_use]
    pub fn global() -> &'static Self {
        GLOBAL_SCANNER.get_or_init(Self::compile)
    }

    /// Scans `content` for potential secrets unless `rel_path` matches an exempt template file.
    #[must_use]
    pub fn scan_text(&self, rel_path: &str, content: &str) -> Vec<SecretFinding> {
        if content.is_empty() || is_secret_scan_exempt_path(rel_path) {
            return Vec::new();
        }
        if !self.fast_set.is_match(content) {
            return Vec::new();
        }

        let mut findings = Vec::new();
        for (zero_idx, line) in content.lines().enumerate() {
            let line_num = zero_idx + 1;
            for rule in &self.rules {
                if !rule.regex.is_match(line) {
                    continue;
                }
                if rule.check_capture_placeholder && all_captures_are_placeholders(rule, line) {
                    continue;
                }
                findings.push(SecretFinding {
                    line: line_num,
                    rule: rule.kind,
                });
                break;
            }
        }
        findings
    }

    fn compile() -> Self {
        let specs: &[(SecretRuleKind, &str, bool)] = &[
            (
                SecretRuleKind::PrivateKey,
                r"-----BEGIN (?:RSA |EC |OPENSSH |DSA |PGP )?PRIVATE KEY-----",
                false,
            ),
            (
                SecretRuleKind::AwsAccessKey,
                r"\b(?:AKIA|ASIA|AROA|AIDA)[0-9A-Z]{16}\b",
                false,
            ),
            (
                SecretRuleKind::GitHubToken,
                r"\b(?:gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,})\b",
                false,
            ),
            (
                SecretRuleKind::GoogleApiKey,
                r"\bAIza[0-9A-Za-z\-_]{35}\b",
                false,
            ),
            (
                SecretRuleKind::SlackToken,
                r"\bxox[bparse]-[0-9A-Za-z\-]{10,}\b",
                false,
            ),
            (
                SecretRuleKind::StripeLiveKey,
                r"\b(?:sk|rk)_live_[0-9A-Za-z]{16,}\b",
                false,
            ),
            (
                SecretRuleKind::AiProviderKey,
                r"\b(?:sk-ant-[A-Za-z0-9\-_]{20,}|sk-proj-[A-Za-z0-9\-_]{20,}|sk-[A-Za-z0-9]{20,})\b",
                false,
            ),
            (
                SecretRuleKind::CredentialUrl,
                concat!(
                    r#"(?i)\b(?:https?|postgres(?:ql)?|mysql|mongodb(?:\+srv)?|redis|amqp)"#,
                    r#"://[^\s:/"']+:([^\s@/"']{4,})@[^\s/"']+"#,
                ),
                true,
            ),
            (
                SecretRuleKind::JwtToken,
                r"\beyJ[A-Za-z0-9\-_]{10,}\.[A-Za-z0-9\-_]{10,}\.[A-Za-z0-9\-_]{10,}\b",
                false,
            ),
            (
                SecretRuleKind::HardcodedSecret,
                concat!(
                    r#"(?i)\b(?:api[_-]?key|secret[_-]?key|access[_-]?token|auth[_-]?token"#,
                    r#"|client[_-]?secret|db[_-]?password|password|private[_-]?key)\b"#,
                    r#"\s*[:=]\s*["']([^"'\r\n]{8,})["']"#,
                ),
                true,
            ),
            (
                SecretRuleKind::HardcodedSecret,
                concat!(
                    r#"\b[A-Z0-9_]*(?:API_KEY|SECRET_KEY|ACCESS_TOKEN|AUTH_TOKEN"#,
                    r#"|CLIENT_SECRET|DB_PASSWORD|PASSWORD|PRIVATE_KEY)[A-Z0-9_]*"#,
                    r#"=([^\s"'#();{}\[\]<>]{8,})"#,
                ),
                true,
            ),
        ];

        let patterns: Vec<&str> = specs.iter().map(|s| s.1).collect();
        let fast_set = RegexSet::new(&patterns).expect("static secret RegexSet must compile");
        let rules = specs
            .iter()
            .map(|&(kind, pat, check_capture_placeholder)| CompiledRule {
                kind,
                regex: Regex::new(pat).expect("static secret Regex must compile"),
                check_capture_placeholder,
            })
            .collect();

        Self { fast_set, rules }
    }
}

/// Scans all readable text files in `tree` and stores finding counts and details per file.
pub fn populate_secret_hits<S: BuildHasher>(
    tree: &mut DirectoryTree,
    contents: &HashMap<NodeId, String, S>,
    scanner: &SecretScanner,
) {
    let work: Vec<(NodeId, &str, &str)> = tree
        .nodes()
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if matches!(node.kind, NodeKind::TextFile(_)) {
                let id = NodeId(idx);
                contents
                    .get(&id)
                    .map(|text| (id, node.rel_path.as_str(), text.as_str()))
            } else {
                None
            }
        })
        .collect();

    let findings_per_node: Vec<(NodeId, Vec<SecretFinding>)> =
        if work.len() < PARALLEL_SCAN_MIN_FILES {
            work.into_iter()
                .map(|(id, path, text)| (id, scanner.scan_text(path, text)))
                .collect()
        } else {
            work.par_iter()
                .map(|&(id, path, text)| (id, scanner.scan_text(path, text)))
                .collect()
        };

    for (id, findings) in findings_per_node {
        tree.set_secret_findings(id, findings);
    }
}

fn all_captures_are_placeholders(rule: &CompiledRule, line: &str) -> bool {
    let mut saw_any = false;
    for caps in rule.regex.captures_iter(line) {
        if let Some(val) = caps.get(1).map(|m| m.as_str()) {
            saw_any = true;
            if !is_placeholder_value(val) {
                return false;
            }
        }
    }
    saw_any
}

fn is_placeholder_value(val: &str) -> bool {
    let trimmed = val.trim();
    if trimmed.starts_with('$')
        || trimmed.starts_with('<')
        || trimmed.starts_with("{{")
        || trimmed.starts_with('[')
        || trimmed.contains("${")
        || trimmed.contains("{{")
        || (trimmed.contains('<') && trimmed.contains('>'))
    {
        return true;
    }

    let lower = trimmed.to_ascii_lowercase();
    let exact_labels = [
        "password",
        "passwort",
        "contraseña",
        "api_key",
        "api key",
        "api-key",
        "api token",
        "x-api-key",
        "secret",
        "secret_key",
        "secret key",
        "access_token",
        "access token",
        "auth_token",
        "auth token",
        "client_secret",
        "client secret",
        "private_key",
        "private key",
        "db_password",
        "passphrase",
        "bearer token",
    ];
    if exact_labels.contains(&lower.as_str()) {
        return true;
    }

    let ui_prefixes = [
        "enter ",
        "input ",
        "type your ",
        "invalid ",
        "missing ",
        "required ",
        "forgot ",
        "reset ",
        "confirm ",
        "please ",
        "provide ",
    ];
    if ui_prefixes.iter().any(|pfx| lower.starts_with(pfx)) {
        return true;
    }

    let placeholder_tokens = [
        "example",
        "changeme",
        "placeholder",
        "your_",
        "your-",
        "dummy",
        "fake",
        "mock",
        "sample",
        "test_only",
        "redacted",
        "undefined",
        "xxx",
        "***",
    ];
    if placeholder_tokens.iter().any(|p| lower.contains(p)) {
        return true;
    }

    let mut chars = trimmed.chars();
    if let Some(first) = chars.next() {
        return chars.all(|c| c == first);
    }
    true
}
