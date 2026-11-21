//! Versioned atomic JSON persistence for per-workspace delta overrides in `%LOCALAPPDATA%`.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

/// Errors returned by strict [`MemoryStore`] read or write operations.
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

/// File-backed store mapping normalized workspace directories to JSON delta files.
#[derive(Debug, Clone)]
pub struct MemoryStore {
    state_dir: PathBuf,
}

impl MemoryStore {
    /// Resolves the default user state directory (`%LOCALAPPDATA%\cpdir\state` on Windows).
    #[must_use]
    pub fn default_system() -> Self {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("XDG_STATE_HOME").map(PathBuf::from))
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("state"))
            })
            .unwrap_or_else(std::env::temp_dir);

        Self {
            state_dir: base.join("cpdir").join("state"),
        }
    }

    /// Creates a store rooted at an explicit directory (for isolated tests without env mutation).
    #[must_use]
    pub fn in_dir(state_dir: impl Into<PathBuf>) -> Self {
        Self {
            state_dir: state_dir.into(),
        }
    }

    /// Returns the deterministic state JSON path associated with `target_dir`.
    #[must_use]
    pub fn state_file_path(&self, target_dir: &Path) -> PathBuf {
        let normalized = normalize_dir_for_hash(target_dir);
        let digest = Sha256::digest(normalized.as_bytes());
        let mut hex = String::with_capacity(32);
        for byte in &digest[..16] {
            let _ = write!(hex, "{byte:02x}");
        }
        self.state_dir.join(format!("{hex}.json"))
    }

    /// Loads saved state for `target_dir`, returning `Ok(None)` if no prior state exists.
    ///
    /// # Errors
    ///
    /// Returns [`StateError`] on I/O failures, malformed JSON, or unsupported schema versions.
    pub fn load(&self, target_dir: &Path) -> Result<Option<SavedWorkspaceState>, StateError> {
        let path = self.state_file_path(target_dir);
        let raw = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(StateError::Io(err.to_string())),
        };
        let parsed: SavedWorkspaceState =
            serde_json::from_str(&raw).map_err(|err| StateError::CorruptJson(err.to_string()))?;
        if parsed.schema_version != STATE_SCHEMA_VERSION {
            return Err(StateError::UnsupportedSchema(parsed.schema_version));
        }
        Ok(Some(parsed))
    }

    /// Loads saved state gracefully, degrading to `None` if disk data is corrupt or outdated.
    #[must_use]
    pub fn load_lenient(&self, target_dir: &Path) -> Option<SavedWorkspaceState> {
        self.load(target_dir).ok().flatten()
    }

    /// Atomically persists `state` for `target_dir`, removing the file if back to clean defaults.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::Io`] if directory creation, serialization, or atomic rename fails.
    pub fn save(&self, target_dir: &Path, state: &SavedWorkspaceState) -> Result<(), StateError> {
        let final_path = self.state_file_path(target_dir);
        if state.is_default_empty() {
            match fs::remove_file(&final_path) {
                Ok(()) => return Ok(()),
                Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(err) => return Err(StateError::Io(err.to_string())),
            }
        }

        fs::create_dir_all(&self.state_dir).map_err(|err| StateError::Io(err.to_string()))?;
        let json =
            serde_json::to_string_pretty(state).map_err(|err| StateError::Io(err.to_string()))?;

        let tmp_name = format!(
            "{}.tmp.{}",
            final_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("state.json"),
            std::process::id()
        );
        let tmp_path = self.state_dir.join(tmp_name);
        if let Err(err) = fs::write(&tmp_path, json) {
            let _ = fs::remove_file(&tmp_path);
            return Err(StateError::Io(err.to_string()));
        }
        if let Err(err) = fs::rename(&tmp_path, &final_path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(StateError::Io(err.to_string()));
        }
        Ok(())
    }
}

/// Normalizes a workspace path consistently across Explorer, PowerShell, and `cmd` invocations.
#[must_use]
pub fn normalize_dir_for_hash(target_dir: &Path) -> String {
    let resolved = fs::canonicalize(target_dir).unwrap_or_else(|_| target_dir.to_path_buf());
    let raw = resolved.to_string_lossy();
    let stripped = raw
        .strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .or_else(|| raw.strip_prefix(r"\\?\").map(str::to_owned))
        .unwrap_or_else(|| raw.into_owned());
    if cfg!(windows) {
        stripped.to_ascii_lowercase()
    } else {
        stripped
    }
}
