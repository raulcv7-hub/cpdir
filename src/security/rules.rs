//! Compiled regex rule specifications for heuristic secret detection.

use regex::{Regex, RegexSet};

use crate::domain::SecretRuleKind;

#[derive(Debug)]
pub(super) struct CompiledRule {
    pub(super) kind: SecretRuleKind,
    pub(super) regex: Regex,
    pub(super) check_capture_placeholder: bool,
}

pub(super) fn compile_secret_rules() -> (RegexSet, Vec<CompiledRule>) {
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

    (fast_set, rules)
}
