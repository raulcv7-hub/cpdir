//! Shared integration test harness for real filesystem, Git, TUI, and strict XML verification.

#![allow(dead_code)]

pub(crate) mod git_repo;
pub(crate) mod scan_helpers;
pub(crate) mod tui_harness;
pub(crate) mod workspace;
pub(crate) mod xml_validator;
