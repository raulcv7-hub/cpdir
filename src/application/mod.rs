//! Application orchestration layer uniting domain state, I/O adapters, and interactive TUI loop.

pub mod analyze;
pub mod error;
mod event_loop;
mod terminal;
pub mod workspace;

use std::io::{self, IsTerminal as _};
use std::path::Path;

use crate::git::GitContextConfig;
use crate::output::ClipboardWriter;
use crate::scanner::{ScanConfig, ScanError};
use crate::state::MemoryStore;

pub use analyze::analyze_tree_contents;
pub use error::{AppError, RunOutcome};
pub use workspace::WorkspaceSession;

/// Executes the full interactive `cpdir` pipeline on `target_dir`.
///
/// # Errors
///
/// Returns [`AppError`] if `target_dir` is invalid, the process lacks a TTY, directory scanning
/// fails, tokenization fails, terminal rendering fails, hydration fails, or clipboard writing
/// fails.
pub fn run_interactive(
    target_dir: &Path,
    include_abs_path: bool,
    clipboard: &dyn ClipboardWriter,
) -> Result<RunOutcome, AppError> {
    if !target_dir.is_dir() {
        return Err(AppError::Scan(ScanError::NotADirectory(
            target_dir.to_path_buf(),
        )));
    }
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err(AppError::NotATerminal);
    }

    let (mut session, mut state) = WorkspaceSession::open(
        target_dir,
        MemoryStore::default_system(),
        ScanConfig::default(),
        GitContextConfig::default(),
    )?;
    state.set_include_abs_path(include_abs_path);
    event_loop::run_tui_event_loop(&mut session, &mut state)?;
    session.export_clipboard(state, clipboard)
}
