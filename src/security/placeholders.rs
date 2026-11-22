//! False-positive filtering for UI/i18n labels, template variables, and placeholder tokens.

const EXACT_LABELS: &[&str] = &[
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

const UI_PREFIXES: &[&str] = &[
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

const PLACEHOLDER_TOKENS: &[&str] = &[
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

pub(super) fn is_placeholder_value(val: &str) -> bool {
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
    if EXACT_LABELS.contains(&lower.as_str()) {
        return true;
    }
    if UI_PREFIXES.iter().any(|pfx| lower.starts_with(pfx)) {
        return true;
    }
    if PLACEHOLDER_TOKENS.iter().any(|p| lower.contains(p)) {
        return true;
    }

    let mut chars = trimmed.chars();
    if let Some(first) = chars.next() {
        return chars.all(|c| c == first);
    }
    true
}
