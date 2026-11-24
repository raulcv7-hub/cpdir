//! Non-blocking startup loader driving the `Loading` view, worker cancellation, and deferred Git.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::application::error::AppError;
use crate::application::ingest::{StreamingIngestOutput, scan_and_ingest_streaming};
use crate::application::terminal::TerminalGuard;
use crate::application::workspace::WorkspaceSession;
use crate::domain::GitContextSnapshot;
use crate::git::{GitContextConfig, extract_git_context_cancelable};
use crate::scanner::ScanConfig;
use crate::scanner::walker::resolve_root_name;
use crate::state::MemoryStore;
use crate::tui::{LoadingPhase, LoadingProgress, TuiAppState, render_loading_view};

const LOADING_POLL_INTERVAL: Duration = Duration::from_millis(35);
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(50);

/// RAII owner for background Git extraction ensuring cooperative cancellation and clean join.
pub(super) struct BackgroundGit {
    cancel: Arc<AtomicBool>,
    rx: Receiver<Option<GitContextSnapshot>>,
    handle: Option<JoinHandle<()>>,
}

impl BackgroundGit {
    pub(super) fn try_recv(&self) -> Result<Option<GitContextSnapshot>, TryRecvError> {
        self.rx.try_recv()
    }
}

impl Drop for BackgroundGit {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub(super) struct LoadedWorkspace {
    pub(super) session: WorkspaceSession,
    pub(super) state: TuiAppState,
    pub(super) bg_git: Option<BackgroundGit>,
}

enum GitResolveState {
    Pending,
    Resolved(Option<GitContextSnapshot>),
}

enum ScanWorkerMsg {
    Progress(LoadingProgress),
    Finished(Result<StreamingIngestOutput, AppError>),
}

struct ScanWorkerGuard {
    cancel: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for ScanWorkerGuard {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Configuration bundle passed to [`load_workspace_interactive`].
pub(super) struct LoadConfig<'a> {
    pub(super) target_dir: &'a Path,
    pub(super) include_abs_path: bool,
    pub(super) store: MemoryStore,
    pub(super) scan_config: ScanConfig,
    pub(super) git_config: GitContextConfig,
}

/// Renders live startup progress inside `guard` while scanning and running Git in background.
pub(super) fn load_workspace_interactive(
    guard: &mut TerminalGuard,
    cfg: &LoadConfig<'_>,
) -> Result<Option<LoadedWorkspace>, AppError> {
    let scan_cancel = Arc::new(AtomicBool::new(false));
    let git_cancel = Arc::new(AtomicBool::new(false));
    let root_name = resolve_root_name(cfg.target_dir);
    let mut progress = LoadingProgress::new(root_name.clone());
    let (mut bg_git, scan_rx, _scan_guard) =
        spawn_workers(cfg, root_name, &scan_cancel, &git_cancel);

    let mut git_state = GitResolveState::Pending;
    let mut needs_draw = true;

    loop {
        poll_background_git(&mut bg_git, &mut git_state, &mut progress, &mut needs_draw);

        if let Some(loaded_res) = drain_scan_messages(&scan_rx, &mut progress, &mut needs_draw) {
            let loaded = loaded_res?;
            if loaded.cancelled {
                return Ok(None);
            }
            progress.phase = LoadingPhase::Finalizing;
            return Ok(Some(finalize_loaded_workspace(
                cfg, loaded, git_state, bg_git,
            )));
        }

        if needs_draw {
            guard
                .terminal_mut()
                .draw(|frame| render_loading_view(frame, frame.area(), &progress))
                .map_err(|err| AppError::TerminalIo(err.to_string()))?;
            needs_draw = false;
        }

        if poll_loading_cancel(&scan_cancel, &git_cancel, &mut needs_draw)? {
            return Ok(None);
        }
    }
}

fn spawn_workers(
    cfg: &LoadConfig<'_>,
    root_name: String,
    scan_cancel: &Arc<AtomicBool>,
    git_cancel: &Arc<AtomicBool>,
) -> (
    Option<BackgroundGit>,
    Receiver<ScanWorkerMsg>,
    ScanWorkerGuard,
) {
    let (git_tx, git_rx) = mpsc::channel();
    let git_dir = cfg.target_dir.to_path_buf();
    let git_worker_cancel = Arc::clone(git_cancel);
    let git_cfg = cfg.git_config;
    let git_handle = thread::spawn(move || {
        let snap = extract_git_context_cancelable(&git_dir, &git_cfg, &git_worker_cancel);
        let _ = git_tx.send(snap);
    });
    let bg_git = Some(BackgroundGit {
        cancel: Arc::clone(git_cancel),
        rx: git_rx,
        handle: Some(git_handle),
    });

    let (scan_tx, scan_rx) = mpsc::channel();
    let scan_dir = cfg.target_dir.to_path_buf();
    let scan_worker_cancel = Arc::clone(scan_cancel);
    let scan_cfg = cfg.scan_config;
    let scan_handle = thread::spawn(move || {
        let mut local_prog = LoadingProgress::new(root_name);
        let mut last_emit: Option<Instant> = None;
        let res = scan_and_ingest_streaming(
            &scan_dir,
            &scan_cfg,
            Some(&scan_worker_cancel),
            &mut local_prog,
            &mut |snapshot| {
                let now = Instant::now();
                if last_emit.is_none_or(|prev| now.duration_since(prev) >= PROGRESS_EMIT_INTERVAL) {
                    let _ = scan_tx.send(ScanWorkerMsg::Progress(snapshot.clone()));
                    last_emit = Some(now);
                }
            },
        );
        let _ = scan_tx.send(ScanWorkerMsg::Finished(res));
    });
    let scan_guard = ScanWorkerGuard {
        cancel: Arc::clone(scan_cancel),
        handle: Some(scan_handle),
    };

    (bg_git, scan_rx, scan_guard)
}

fn poll_background_git(
    bg_git: &mut Option<BackgroundGit>,
    git_state: &mut GitResolveState,
    progress: &mut LoadingProgress,
    needs_draw: &mut bool,
) {
    if !matches!(git_state, GitResolveState::Pending) {
        return;
    }
    let Some(bg) = bg_git.as_ref() else {
        return;
    };
    match bg.try_recv() {
        Ok(snap) => {
            progress.git_pending = false;
            *git_state = GitResolveState::Resolved(snap);
            *bg_git = None;
            *needs_draw = true;
        }
        Err(TryRecvError::Disconnected) => {
            progress.git_pending = false;
            *git_state = GitResolveState::Resolved(None);
            *bg_git = None;
            *needs_draw = true;
        }
        Err(TryRecvError::Empty) => {}
    }
}

fn finalize_loaded_workspace(
    cfg: &LoadConfig<'_>,
    loaded: StreamingIngestOutput,
    git_state: GitResolveState,
    bg_git: Option<BackgroundGit>,
) -> LoadedWorkspace {
    let (git_still_pending, git_snapshot) = match git_state {
        GitResolveState::Pending => (bg_git.is_some(), None),
        GitResolveState::Resolved(snap) => (false, snap),
    };
    let (session, mut state) = WorkspaceSession::from_loaded(
        cfg.target_dir,
        cfg.store.clone(),
        cfg.scan_config,
        loaded,
        git_snapshot,
    );
    if cfg.include_abs_path {
        state.set_include_abs_path(true);
        session.refresh_xml_overhead(&mut state);
    }
    if git_still_pending {
        state.set_git_pending(true);
    }
    LoadedWorkspace {
        session,
        state,
        bg_git,
    }
}

fn poll_loading_cancel(
    scan_cancel: &Arc<AtomicBool>,
    git_cancel: &Arc<AtomicBool>,
    needs_draw: &mut bool,
) -> Result<bool, AppError> {
    if event::poll(LOADING_POLL_INTERVAL).map_err(|err| AppError::TerminalIo(err.to_string()))? {
        while event::poll(Duration::ZERO).map_err(|err| AppError::TerminalIo(err.to_string()))? {
            let ev = event::read().map_err(|err| AppError::TerminalIo(err.to_string()))?;
            if is_cancel_event(&ev) {
                scan_cancel.store(true, Ordering::Release);
                git_cancel.store(true, Ordering::Release);
                return Ok(true);
            }
            if matches!(ev, Event::Resize(_, _)) {
                *needs_draw = true;
            }
        }
    }
    Ok(false)
}

fn drain_scan_messages(
    rx: &Receiver<ScanWorkerMsg>,
    progress: &mut LoadingProgress,
    needs_draw: &mut bool,
) -> Option<Result<StreamingIngestOutput, AppError>> {
    let mut finished = None;
    while let Ok(msg) = rx.try_recv() {
        match msg {
            ScanWorkerMsg::Progress(next) => {
                let git_pending = progress.git_pending;
                *progress = next;
                progress.git_pending = git_pending;
                *needs_draw = true;
            }
            ScanWorkerMsg::Finished(res) => {
                finished = Some(res);
                break;
            }
        }
    }
    finished
}

fn is_cancel_event(ev: &Event) -> bool {
    let Event::Key(key) = ev else {
        return false;
    };
    if key.kind == KeyEventKind::Release {
        return false;
    }
    matches!(key.code, KeyCode::Esc | KeyCode::Char('q' | 'Q'))
        || (matches!(key.code, KeyCode::Char('c' | 'C'))
            && key.modifiers.contains(KeyModifiers::CONTROL))
}
