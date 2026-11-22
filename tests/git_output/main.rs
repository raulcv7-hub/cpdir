//! Git context extraction, strict XML golden serialization, and `proptest` round-trip suite.

#[path = "../common/mod.rs"]
mod common;

mod git_hardening;
mod git_history;
mod git_monorepo;
mod xml_golden;
mod xml_proptest;
mod xml_sanitization;
