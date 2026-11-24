//! Application orchestration layer uniting domain state, I/O adapters, and interactive TUI loop.

pub mod analyze;
pub mod error;
mod event_loop;
mod ingest;
mod loader;
mod terminal;
pub mod workspace;

use std::io::{self, IsTerminal as _};
use std::path::Path;

use crate::application::loader::LoadConfig;
use crate::application::terminal::TerminalGuard;
use crate::git::GitContextConfig;
use crate::output::ClipboardWriter;
use crate::scanner::{ScanConfig, ScanError};
use crate::state::MemoryStore;

pub use analyze::analyze_tree_contents;
pub use error::{AppError, RunOutcome};
pub use workspace::WorkspaceSession;

/// Executes the full interactive `cpdir` pipeline on `target_dir` without blocking terminal open.
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

    let mut guard = TerminalGuard::enter()?;
    let Some(mut loaded) = loader::load_workspace_interactive(
        &mut guard,
        &LoadConfig {
            target_dir,
            include_abs_path,
            store: MemoryStore::default_system(),
            scan_config: ScanConfig::default(),
            git_config: GitContextConfig::default(),
        },
    )?
    else {
        return Ok(RunOutcome::Cancelled);
    };

    event_loop::run_tui_event_loop(
        &mut guard,
        &mut loaded.session,
        &mut loaded.state,
        loaded.bg_git,
    )?;
    loaded.session.export_clipboard(loaded.state, clipboard)
}
