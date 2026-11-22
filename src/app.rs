//! Imperative application shell orchestrating scan, tokenization, TUI session, and clipboard.

use std::collections::HashMap;
use std::fmt;
use std::io::{self, BufWriter, IsTerminal as _, Write as _};
use std::path::Path;
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use crate::config::defaults::{DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD, HARD_READ_BYTE_CAP};
use crate::domain::NodeId;
use crate::git::{GitContextConfig, extract_git_context};
use crate::output::{
    ClipboardError, ClipboardWriter, XmlWriteError, render_git_context_xml, render_strict_xml,
};
use crate::scanner::{
    ContentHydrationError, DEFAULT_RESCAN_MAX_ENTRIES, INLINE_CONTENT_CACHE_BYTES, ScanConfig,
    ScanError, ScanResult, hydrate_selected_contents, release_large_unselected_contents,
    rescan_pruned_dir, scan_directory,
};
use crate::security::{SecretScanner, populate_secret_hits};
use crate::state::{MemoryStore, SavedWorkspaceState, normalize_dir_for_hash};
use crate::tokens::{TokenCounter, TokenError, populate_token_counts};
use crate::tui::{
    AppExitAction, TuiAppState, handle_key_event, handle_mouse_event, render_active_modal,
    render_tree_view,
};

/// Successful interactive run result returned to `main`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// Strict XML was copied to the OS clipboard.
    Copied {
        /// Number of selected files included in `<files>`.
        files: usize,
        /// Total selected BPE tokens across included files and active git context.
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
    /// Selected file could not be reloaded from disk prior to XML emission.
    ContentHydration(ContentHydrationError),
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
            Self::ContentHydration(err) => write!(f, "{err}"),
            Self::TerminalIo(reason) => write!(f, "terminal error: {reason}"),
            Self::Xml(err) => write!(f, "{err}"),
            Self::Clipboard(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for AppError {}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<BufWriter<io::Stderr>>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self, AppError> {
        enable_raw_mode().map_err(|err| AppError::TerminalIo(err.to_string()))?;
        let mut stderr = io::stderr();
        if let Err(err) = execute!(stderr, EnterAlternateScreen, event::EnableMouseCapture) {
            let _ = disable_raw_mode();
            return Err(AppError::TerminalIo(err.to_string()));
        }
        let buffered = BufWriter::with_capacity(32 * 1024, stderr);
        let backend = CrosstermBackend::new(buffered);
        match Terminal::new(backend) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(err) => {
                let _ = execute!(
                    io::stderr(),
                    event::DisableMouseCapture,
                    LeaveAlternateScreen
                );
                let _ = disable_raw_mode();
                Err(AppError::TerminalIo(err.to_string()))
            }
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.terminal.show_cursor();
        let _ = execute!(
            self.terminal.backend_mut(),
            event::DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = self.terminal.backend_mut().flush();
        let _ = disable_raw_mode();
    }
}

/// Executes the full interactive `cpdir` pipeline on `target_dir`.
///
/// # Errors
///
/// Returns [`AppError`] if the process lacks a TTY, directory scanning fails, tokenization fails,
/// terminal rendering fails, hydration fails, or clipboard writing fails.
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

    // Run independent Git CLI subprocesses concurrently with filesystem walk and tokenization.
    let (scan_res, git_snapshot) = std::thread::scope(|scope| {
        let git_handle =
            scope.spawn(|| extract_git_context(target_dir, &GitContextConfig::default()));
        let scan_res = scan_and_analyze(target_dir);
        let git_snapshot = git_handle.join().unwrap_or(None);
        (scan_res, git_snapshot)
    });
    let mut scan = scan_res?;

    let store = MemoryStore::default_system();
    let saved = store.load_lenient(target_dir);
    if let Some(prev) = &saved {
        scan.tree.apply_overrides(&prev.overrides);
    }
    let _ = release_large_unselected_contents(
        &scan.tree,
        &mut scan.contents,
        INLINE_CONTENT_CACHE_BYTES,
    );

    let mut state = TuiAppState::new(scan.tree, git_snapshot.is_some());
    if let Some(snap) = &git_snapshot
        && let Ok(counter) = TokenCounter::o200k()
    {
        let git_xml = render_git_context_xml(snap);
        state.set_git_context_tokens(counter.count(&git_xml));
    }
    if let Some(prev) = saved
        && let Some(git_enabled) = prev.git_context_enabled
    {
        state.set_include_git_context(git_enabled);
    }

    run_tui_event_loop(target_dir, &mut state, &mut scan.contents)?;

    match state.exit_action() {
        Some(AppExitAction::CopyToClipboard) => {
            let git_disabled = state.has_git_context() && !state.include_git_context();
            let git_persist = if git_disabled { Some(false) } else { None };
            let active_git = if state.include_git_context() {
                git_snapshot.as_ref()
            } else {
                None
            };
            let total_tokens = state.total_effective_tokens();

            let tree = state.into_tree();
            hydrate_selected_contents(target_dir, &tree, &mut scan.contents, HARD_READ_BYTE_CAP)
                .map_err(AppError::ContentHydration)?;
            let counts = tree.compute_metrics().counts;
            let xml =
                render_strict_xml(&tree, &scan.contents, active_git).map_err(AppError::Xml)?;
            clipboard.copy_text(&xml).map_err(AppError::Clipboard)?;

            let to_save = SavedWorkspaceState::new(
                normalize_dir_for_hash(target_dir),
                tree.collect_overrides(),
                git_persist,
            );
            // Best-effort persistence: never fail clipboard confirmation over AppData I/O.
            let _ = store.save(target_dir, &to_save);

            Ok(RunOutcome::Copied {
                files: counts.read_files,
                tokens: total_tokens,
                xml_bytes: xml.len(),
            })
        }
        Some(AppExitAction::Cancelled) | None => Ok(RunOutcome::Cancelled),
    }
}

fn scan_and_analyze(target_dir: &Path) -> Result<ScanResult, AppError> {
    let mut scan = scan_directory(target_dir, &ScanConfig::default()).map_err(AppError::Scan)?;
    let counter = TokenCounter::o200k().map_err(AppError::Tokens)?;
    populate_token_counts(
        &mut scan.tree,
        &scan.contents,
        DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD,
        counter,
    );
    populate_secret_hits(&mut scan.tree, &scan.contents, SecretScanner::global());
    Ok(scan)
}

fn run_tui_event_loop(
    target_dir: &Path,
    state: &mut TuiAppState,
    contents: &mut HashMap<NodeId, String>,
) -> Result<(), AppError> {
    let mut guard = TerminalGuard::enter()?;
    let mut needs_draw = true;

    while state.exit_action().is_none() {
        if needs_draw {
            guard
                .terminal
                .draw(|frame| {
                    let area = frame.area();
                    render_tree_view(frame, area, state);
                    render_active_modal(frame, area, state);
                })
                .map_err(|err| AppError::TerminalIo(err.to_string()))?;
            needs_draw = false;
        }

        let first = event::read().map_err(|err| AppError::TerminalIo(err.to_string()))?;
        let mut reingest_needed = dispatch_app_event(state, &first, &mut needs_draw);

        while state.exit_action().is_none()
            && event::poll(Duration::ZERO).map_err(|err| AppError::TerminalIo(err.to_string()))?
        {
            let next = event::read().map_err(|err| AppError::TerminalIo(err.to_string()))?;
            if dispatch_app_event(state, &next, &mut needs_draw) {
                reingest_needed = true;
            }
        }

        if state.exit_action().is_none() && reingest_needed {
            handle_pending_reingest(target_dir, state, contents);
            needs_draw = true;
        }
    }
    Ok(())
}

fn dispatch_app_event(state: &mut TuiAppState, ev: &Event, needs_draw: &mut bool) -> bool {
    match ev {
        Event::Key(key) => {
            if key.kind != KeyEventKind::Release {
                handle_key_event(state, *key);
                *needs_draw = true;
            }
        }
        Event::Mouse(mouse) => {
            if handle_mouse_event(state, *mouse) {
                *needs_draw = true;
            }
        }
        Event::Resize(_, _) => {
            *needs_draw = true;
        }
        _ => {}
    }
    state.has_pending_reingest()
}

fn handle_pending_reingest(
    target_dir: &Path,
    state: &mut TuiAppState,
    contents: &mut HashMap<NodeId, String>,
) {
    let Some(rel_path) = state.take_pending_reingest() else {
        return;
    };
    let mut sub = match rescan_pruned_dir(
        target_dir,
        &rel_path,
        &ScanConfig::default(),
        DEFAULT_RESCAN_MAX_ENTRIES,
    ) {
        Ok(res) => res,
        Err(err) => {
            state.set_inspector_status(format!("Failed to load {rel_path}/: {err}"));
            return;
        }
    };

    if let Ok(counter) = TokenCounter::o200k() {
        populate_token_counts(
            &mut sub.tree,
            &sub.contents,
            DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD,
            counter,
        );
    }
    populate_secret_hits(&mut sub.tree, &sub.contents, SecretScanner::global());

    if let Some(outcome) = state.apply_reingested_dir(&rel_path, sub.tree, sub.truncated) {
        for (old_id, new_id) in outcome.id_map {
            if let Some(text) = sub.contents.remove(&old_id) {
                contents.insert(new_id, text);
            }
        }
        let _ =
            release_large_unselected_contents(state.tree(), contents, INLINE_CONTENT_CACHE_BYTES);
    }
}
