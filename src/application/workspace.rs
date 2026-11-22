//! Injectable workspace session coordinating scan, git, memory persistence, hydration, and copy.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::application::analyze::{
    analyze_tree_contents, build_initial_tui_state, scan_and_analyze,
};
use crate::application::error::{AppError, RunOutcome};
use crate::config::limits::INLINE_CONTENT_CACHE_BYTES;
use crate::domain::{GitContextSnapshot, NodeId};
use crate::git::{GitContextConfig, extract_git_context};
use crate::output::{ClipboardWriter, render_strict_xml};
use crate::scanner::{
    ScanConfig, ScanError, hydrate_selected_contents, release_large_unselected_contents,
    rescan_pruned_dir,
};
use crate::state::{MemoryStore, SavedWorkspaceState, normalize_dir_for_hash};
use crate::tui::{AppExitAction, TuiAppState};

/// Imperative session owning loaded file buffers, Git metadata, and persistence adapters.
#[derive(Debug)]
pub struct WorkspaceSession {
    target_dir: PathBuf,
    contents: HashMap<NodeId, String>,
    git_snapshot: Option<GitContextSnapshot>,
    scan_config: ScanConfig,
    store: MemoryStore,
}

impl WorkspaceSession {
    /// Scans `target_dir`, extracts scoped Git context concurrently, and initializes [`TuiAppState`].
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Scan`] if `target_dir` is unreadable or [`AppError::Tokens`] if
    /// offline tokenization fails.
    pub fn open(
        target_dir: &Path,
        store: MemoryStore,
        scan_config: ScanConfig,
        git_config: GitContextConfig,
    ) -> Result<(Self, TuiAppState), AppError> {
        if !target_dir.is_dir() {
            return Err(AppError::Scan(ScanError::NotADirectory(
                target_dir.to_path_buf(),
            )));
        }

        // Run independent Git CLI queries concurrently with filesystem traversal and analysis.
        let (scan_res, git_snapshot) = std::thread::scope(|scope| {
            let git_handle = scope.spawn(|| extract_git_context(target_dir, &git_config));
            let scan_res = scan_and_analyze(target_dir, &scan_config);
            let git_snapshot = git_handle.join().unwrap_or(None);
            (scan_res, git_snapshot)
        });
        let mut scan = scan_res?;

        let saved = store.load_lenient(target_dir);
        if let Some(prev) = &saved {
            scan.tree.apply_overrides(&prev.overrides);
        }
        let _ = release_large_unselected_contents(
            &scan.tree,
            &mut scan.contents,
            INLINE_CONTENT_CACHE_BYTES,
        );

        let state = build_initial_tui_state(scan.tree, git_snapshot.as_ref(), saved.as_ref());
        let session = Self {
            target_dir: target_dir.to_path_buf(),
            contents: scan.contents,
            git_snapshot,
            scan_config,
            store,
        };
        Ok((session, state))
    }

    /// Returns an immutable reference to currently cached file contents.
    #[must_use]
    pub const fn contents(&self) -> &HashMap<NodeId, String> {
        &self.contents
    }

    /// Returns the extracted Git context snapshot if the directory is inside a repository.
    #[must_use]
    pub const fn git_snapshot(&self) -> Option<&GitContextSnapshot> {
        self.git_snapshot.as_ref()
    }

    /// Executes any pending pruned-directory re-scan requested by the Inspector, returning `true`
    /// when a request was processed.
    pub fn reingest_pending(&mut self, state: &mut TuiAppState, max_entries: usize) -> bool {
        let Some(rel_path) = state.take_pending_reingest() else {
            return false;
        };

        let mut sub =
            match rescan_pruned_dir(&self.target_dir, &rel_path, &self.scan_config, max_entries) {
                Ok(res) => res,
                Err(err) => {
                    state.set_inspector_status(format!("Failed to load {rel_path}/: {err}"));
                    return true;
                }
            };

        let _ = analyze_tree_contents(&mut sub.tree, &sub.contents);

        if let Some(outcome) = state.apply_reingested_dir(&rel_path, sub.tree, sub.truncated) {
            for (old_id, new_id) in outcome.id_map {
                if let Some(text) = sub.contents.remove(&old_id) {
                    self.contents.insert(new_id, text);
                }
            }
            let _ = release_large_unselected_contents(
                state.tree(),
                &mut self.contents,
                INLINE_CONTENT_CACHE_BYTES,
            );
        }
        true
    }

    /// Hydrates selected files, renders strict XML, copies to `clipboard`, and persists deltas.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::ContentHydration`], [`AppError::Xml`], or [`AppError::Clipboard`] if
    /// hydration, serialization, or clipboard access fails.
    pub fn export_clipboard(
        &mut self,
        state: TuiAppState,
        clipboard: &dyn ClipboardWriter,
    ) -> Result<RunOutcome, AppError> {
        if !matches!(state.exit_action(), Some(AppExitAction::CopyToClipboard)) {
            return Ok(RunOutcome::Cancelled);
        }

        let git_disabled = state.has_git_context() && !state.include_git_context();
        let git_persist = if git_disabled { Some(false) } else { None };
        let active_git = if state.include_git_context() {
            self.git_snapshot.as_ref()
        } else {
            None
        };
        let total_tokens = state.total_effective_tokens();

        let tree = state.into_tree();
        hydrate_selected_contents(
            &self.target_dir,
            &tree,
            &mut self.contents,
            self.scan_config.max_file_bytes,
        )
        .map_err(AppError::ContentHydration)?;

        let counts = tree.compute_metrics().counts;
        let xml = render_strict_xml(&tree, &self.contents, active_git).map_err(AppError::Xml)?;
        clipboard.copy_text(&xml).map_err(AppError::Clipboard)?;

        let to_save = SavedWorkspaceState::new(
            normalize_dir_for_hash(&self.target_dir),
            tree.collect_overrides(),
            git_persist,
        );
        // Best-effort persistence: never fail clipboard confirmation over AppData I/O.
        let _ = self.store.save(&self.target_dir, &to_save);

        Ok(RunOutcome::Copied {
            files: counts.read_files,
            tokens: total_tokens,
            xml_bytes: xml.len(),
        })
    }
}
