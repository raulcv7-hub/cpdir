//! Versioned JSON state schema and error types for per-workspace delta persistence.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Current on-disk JSON schema version for Expand/Contract evolution safety.
pub const STATE_SCHEMA_VERSION: u32 = 1;

/// Persisted selection deltas and optional toggles for a single workspace directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedWorkspaceState {
    /// Schema revision identifier (`1`).
    pub schema_version: u32,
    /// Human-readable normalized workspace path for diagnostics.
    #[serde(default)]
    pub workspace_path: String,
    /// Minimal delta map of explicit user overrides (`false` = excluded, `true` = included).
    #[serde(default)]
    pub overrides: BTreeMap<String, bool>,
    /// Persisted `<git_context>` toggle when explicitly disabled (`Some(false)`).
    #[serde(default)]
    pub git_context_enabled: Option<bool>,
}

impl SavedWorkspaceState {
    /// Creates a state payload ready to persist for `workspace_path`.
    #[must_use]
    pub fn new(
        workspace_path: impl Into<String>,
        overrides: BTreeMap<String, bool>,
        git_context_enabled: Option<bool>,
    ) -> Self {
        Self {
            schema_version: STATE_SCHEMA_VERSION,
            workspace_path: workspace_path.into(),
            overrides,
            git_context_enabled,
        }
    }

    /// Returns `true` when there are no non-default overrides or toggles left to store.
    #[must_use]
    pub fn is_default_empty(&self) -> bool {
        self.overrides.is_empty() && self.git_context_enabled.is_none()
    }
}

/// Errors returned by strict [`crate::state::MemoryStore`] read or write operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    /// Filesystem I/O failure creating directories or writing/reading state files.
    Io(String),
    /// Saved JSON payload is corrupted or unparseable.
    CorruptJson(String),
    /// Saved JSON carries an incompatible `schema_version`.
    UnsupportedSchema(u32),
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "state I/O error: {msg}"),
            Self::CorruptJson(msg) => write!(f, "corrupt state JSON: {msg}"),
            Self::UnsupportedSchema(ver) => write!(f, "unsupported state schema version: {ver}"),
        }
    }
}

impl std::error::Error for StateError {}
