//! Pure linear-time heuristic secret scanner decoupled from domain tree structures.

use std::sync::OnceLock;

use rayon::prelude::*;
use regex::RegexSet;

use crate::config::rules::is_secret_scan_exempt_path;
use crate::domain::SecretFinding;
use crate::security::placeholders::is_placeholder_value;
use crate::security::rules::{CompiledRule, compile_secret_rules};

const PARALLEL_SCAN_MIN_FILES: usize = 8;

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
        GLOBAL_SCANNER.get_or_init(|| {
            let (fast_set, rules) = compile_secret_rules();
            Self { fast_set, rules }
        })
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

    /// Scans `(key, rel_path, content)` items sequentially or in parallel via `rayon`.
    #[must_use]
    pub fn scan_batch<K: Copy + Send + Sync>(
        &self,
        items: &[(K, &str, &str)],
    ) -> Vec<(K, Vec<SecretFinding>)> {
        if items.len() < PARALLEL_SCAN_MIN_FILES {
            items
                .iter()
                .map(|&(key, path, text)| (key, self.scan_text(path, text)))
                .collect()
        } else {
            items
                .par_iter()
                .map(|&(key, path, text)| (key, self.scan_text(path, text)))
                .collect()
        }
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
