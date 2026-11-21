//! Imperative application shell orchestrating scan, tokenization, TUI session, and clipboard.

use std::fmt;
use std::io::{self, IsTerminal as _};
use std::path::Path;

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use crate::config::defaults::DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD;
use crate::output::{ClipboardError, ClipboardWriter, XmlWriteError, render_strict_xml};
use crate::scanner::{ScanConfig, ScanError, scan_directory};
use crate::tokens::{TokenCounter, TokenError, populate_token_counts};
use crate::tui::{AppExitAction, TuiAppState, handle_key_event, render_tree_view};

/// Successful interactive run result returned to `main`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// Strict XML was copied to the OS clipboard.
    Copied {
        /// Number of selected files included in `<files>`.
        files: usize,
        /// Total selected BPE tokens across included files.
        tokens: usize,
        /// Final UTF-8 XML payload size in bytes.
        xml_bytes: usize,
    },
    /// User aborted the interactive session without modifying the clipboard.
    Cancelled,
}

/// Errors emitted across the interactive `cpdir` execution pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    /// Standard input or standard error is not attached to an interactive terminal.
    NotATerminal,
    /// Directory scan failure.
    Scan(ScanError),
    /// Offline tokenizer initialization failure.
    Tokens(TokenError),
    /// Terminal I/O or raw-mode failure.
    TerminalIo(String),
    /// Strict XML serialization error.
    Xml(XmlWriteError),
    /// OS clipboard error.
    Clipboard(ClipboardError),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotATerminal => write!(f, "cpdir requires an interactive terminal"),
            Self::Scan(err) => write!(f, "{err}"),
            Self::Tokens(err) => write!(f, "{err}"),
            Self::TerminalIo(reason) => write!(f, "terminal error: {reason}"),
            Self::Xml(err) => write!(f, "{err}"),
            Self::Clipboard(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for AppError {}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stderr>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self, AppError> {
        enable_raw_mode().map_err(|err| AppError::TerminalIo(err.to_string()))?;
        let mut stderr = io::stderr();
        if let Err(err) = execute!(stderr, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(AppError::TerminalIo(err.to_string()));
        }
        let backend = CrosstermBackend::new(stderr);
        match Terminal::new(backend) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(err) => {
                let _ = execute!(io::stderr(), LeaveAlternateScreen);
                let _ = disable_raw_mode();
                Err(AppError::TerminalIo(err.to_string()))
            }
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
        let _ = self.terminal.show_cursor();
    }
}

/// Executes the full interactive `cpdir` pipeline on `target_dir`.
///
/// # Errors
///
/// Returns [`AppError`] if the process lacks a TTY, directory scanning fails, tokenization fails,
/// terminal rendering fails, or clipboard writing fails.
pub fn run_interactive(
    target_dir: &Path,
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

    let mut scan = scan_directory(target_dir, &ScanConfig::default()).map_err(AppError::Scan)?;
    let counter = TokenCounter::o200k().map_err(AppError::Tokens)?;
    populate_token_counts(
        &mut scan.tree,
        &scan.contents,
        DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD,
        counter,
    );

    let mut state = TuiAppState::new(scan.tree, false);
    run_tui_event_loop(&mut state)?;

    match state.exit_action() {
        Some(AppExitAction::CopyToClipboard) => {
            let tree = state.into_tree();
            let counts = tree.compute_metrics().counts;
            let xml = render_strict_xml(&tree, &scan.contents, None).map_err(AppError::Xml)?;
            clipboard.copy_text(&xml).map_err(AppError::Clipboard)?;
            Ok(RunOutcome::Copied {
                files: counts.read_files,
                tokens: counts.selected_tokens,
                xml_bytes: xml.len(),
            })
        }
        Some(AppExitAction::Cancelled) | None => Ok(RunOutcome::Cancelled),
    }
}

fn run_tui_event_loop(state: &mut TuiAppState) -> Result<(), AppError> {
    let mut guard = TerminalGuard::enter()?;
    while state.exit_action().is_none() {
        guard
            .terminal
            .draw(|frame| render_tree_view(frame, frame.area(), state))
            .map_err(|err| AppError::TerminalIo(err.to_string()))?;

        match event::read() {
            Ok(Event::Key(key)) => handle_key_event(state, key),
            Ok(_) => {}
            Err(err) => return Err(AppError::TerminalIo(err.to_string())),
        }
    }
    Ok(())
}
