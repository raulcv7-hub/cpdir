//! Injectable workspace session coordinating scan, git, memory persistence, hydration, and copy.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::application::analyze::{
    analyze_tree_contents, build_initial_tui_state, compute_selected_xml_delta,
    compute_xml_overhead_tokens,
};
use crate::application::error::{AppError, RunOutcome};
use crate::application::ingest::{StreamingIngestOutput, scan_and_ingest_streaming};
use crate::config::limits::{DEFAULT_RESCAN_MAX_ENTRIES, INLINE_CONTENT_CACHE_BYTES};
use crate::domain::{DirectoryTree, GitContextSnapshot, NodeId, NodeKind};
use crate::git::{GitContextConfig, extract_git_context};
use crate::output::{
    ClipboardWriter, XmlRenderOptions, render_git_context_xml, render_strict_xml_with_options,
};
use crate::scanner::{
    ScanConfig, ScanError, hydrate_selected_contents, release_large_unselected_contents,
    rescan_pruned_dir,
};
use crate::state::{
    MemoryStore, SavedWorkspaceState, normalize_dir_for_hash, resolve_display_abs_path,
};
use crate::tokens::TokenCounter;
use crate::tui::{AppExitAction, LoadingProgress, TuiAppState};

/// Imperative session owning loaded file buffers, Git metadata, and persistence adapters.
#[derive(Debug)]
pub struct WorkspaceSession {
    target_dir: PathBuf,
    display_abs_path: String,
    contents: HashMap<NodeId, String>,
    xml_file_deltas: HashMap<NodeId, isize>,
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

        let mut dummy_progress = LoadingProgress::new("");
        let (ingest_res, git_snapshot) = std::thread::scope(|scope| {
            let git_handle = scope.spawn(|| extract_git_context(target_dir, &git_config));
            let ingest_res = scan_and_ingest_streaming(
                target_dir,
                &scan_config,
                None,
                &mut dummy_progress,
                &mut |_| {},
            );
            let git_snapshot = git_handle.join().unwrap_or(None);
            (ingest_res, git_snapshot)
        });
        let loaded = ingest_res?;
        Ok(Self::from_loaded(
            target_dir,
            store,
            scan_config,
            loaded,
            git_snapshot,
        ))
    }

    pub(super) fn from_loaded(
        target_dir: &Path,
        store: MemoryStore,
        scan_config: ScanConfig,
        mut loaded: StreamingIngestOutput,
        git_snapshot: Option<GitContextSnapshot>,
    ) -> (Self, TuiAppState) {
        let saved = store.load_lenient(target_dir);
        if let Some(prev) = &saved {
            auto_reingest_saved_pruned_dirs(
                target_dir,
                &scan_config,
                &prev.overrides,
                &mut loaded.tree,
                &mut loaded.contents,
                &mut loaded.xml_file_deltas,
            );
            loaded.tree.apply_overrides(&prev.overrides);
            let _ = release_large_unselected_contents(
                &loaded.tree,
                &mut loaded.contents,
                INLINE_CONTENT_CACHE_BYTES,
            );
        }

        let mut state = build_initial_tui_state(loaded.tree, git_snapshot.as_ref(), saved.as_ref());
        let session = Self {
            target_dir: target_dir.to_path_buf(),
            display_abs_path: resolve_display_abs_path(target_dir),
            contents: loaded.contents,
            xml_file_deltas: loaded.xml_file_deltas,
            git_snapshot,
            scan_config,
            store,
        };
        session.refresh_xml_overhead(&mut state);
        (session, state)
    }

    /// Attaches background-resolved Git metadata onto an active session and [`TuiAppState`].
    pub fn attach_git_snapshot(
        &mut self,
        state: &mut TuiAppState,
        git_snapshot: Option<GitContextSnapshot>,
    ) {
        let Some(snap) = git_snapshot else {
            state.set_git_pending(false);
            return;
        };
        let git_tokens = TokenCounter::o200k().map_or(0, |counter| {
            let xml = render_git_context_xml(&snap);
            counter.count(&xml)
        });
        let saved_git = self
            .store
            .load_lenient(&self.target_dir)
            .and_then(|prev| prev.git_context_enabled);
        state.attach_git_context(git_tokens, saved_git);
        self.git_snapshot = Some(snap);
    }

    /// Recomputes XML summary, directory structure, and file-tag token overhead for `state`.
    pub fn refresh_xml_overhead(&self, state: &mut TuiAppState) {
        let abs_root_path = state
            .include_abs_path()
            .then_some(self.display_abs_path.as_str());
        if let Ok(tokens) =
            compute_xml_overhead_tokens(state.tree(), abs_root_path, &self.xml_file_deltas)
        {
            state.set_xml_overhead_tokens(tokens);
        }
    }

    /// Returns the normalized absolute path used when opt-in `root_path` emission is enabled.
    #[must_use]
    pub fn display_abs_path(&self) -> &str {
        &self.display_abs_path
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

        if let Err(err) = analyze_tree_contents(&mut sub.tree, &sub.contents) {
            state.set_inspector_status(format!("Failed to analyze {rel_path}/: {err}"));
            return true;
        }

        let counter = TokenCounter::o200k().ok();
        if let Some(outcome) = state.apply_reingested_dir(&rel_path, sub.tree, sub.truncated) {
            for (old_id, new_id) in outcome.id_map {
                if let Some(text) = sub.contents.remove(&old_id) {
                    if let Some(c) = counter
                        && let Some(node) = state.tree().get(new_id)
                        && let NodeKind::TextFile(file) = &node.kind
                    {
                        let delta =
                            compute_selected_xml_delta(c, &node.rel_path, &text, file.tokens);
                        self.xml_file_deltas.insert(new_id, delta);
                    }
                    self.contents.insert(new_id, text);
                }
            }
            let _ = release_large_unselected_contents(
                state.tree(),
                &mut self.contents,
                INLINE_CONTENT_CACHE_BYTES,
            );
            self.refresh_xml_overhead(state);
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
        let abs_root_path = state
            .include_abs_path()
            .then_some(self.display_abs_path.as_str());
        let fallback_tokens = state.total_effective_tokens();

        let tree = state.into_tree();
        hydrate_selected_contents(
            &self.target_dir,
            &tree,
            &mut self.contents,
            self.scan_config.max_file_bytes,
        )
        .map_err(AppError::ContentHydration)?;

        let counts = tree.compute_metrics().counts;
        let xml = render_strict_xml_with_options(
            &tree,
            &self.contents,
            XmlRenderOptions {
                git_context: active_git,
                abs_root_path,
            },
        )
        .map_err(AppError::Xml)?;
        clipboard.copy_text(&xml).map_err(AppError::Clipboard)?;

        let exact_tokens = TokenCounter::o200k().map_or(fallback_tokens, |c| c.count(&xml));
        let to_save = SavedWorkspaceState::new(
            normalize_dir_for_hash(&self.target_dir),
            tree.collect_overrides(),
            git_persist,
        );
        // Best-effort persistence: never fail clipboard confirmation over AppData I/O.
        let _ = self.store.save(&self.target_dir, &to_save);

        Ok(RunOutcome::Copied {
            files: counts.read_files,
            tokens: exact_tokens,
            xml_bytes: xml.len(),
        })
    }
}

fn auto_reingest_saved_pruned_dirs(
    target_dir: &Path,
    scan_config: &ScanConfig,
    overrides: &BTreeMap<String, bool>,
    tree: &mut DirectoryTree,
    contents: &mut HashMap<NodeId, String>,
    xml_deltas: &mut HashMap<NodeId, isize>,
) {
    let candidates: Vec<String> = tree
        .pruned_dirs()
        .iter()
        .filter(|pruned| {
            let prefix = format!("{}/", pruned.rel_path);
            overrides.iter().any(|(path, &enabled)| {
                enabled && (path == &pruned.rel_path || path.starts_with(&prefix))
            })
        })
        .map(|pruned| pruned.rel_path.clone())
        .collect();

    let counter = TokenCounter::o200k().ok();
    for rel_path in candidates {
        let Ok(mut sub) = rescan_pruned_dir(
            target_dir,
            &rel_path,
            scan_config,
            DEFAULT_RESCAN_MAX_ENTRIES,
        ) else {
            continue;
        };
        if analyze_tree_contents(&mut sub.tree, &sub.contents).is_err() {
            continue;
        }
        if let Some(outcome) = tree.merge_pruned_subtree(&rel_path, sub.tree) {
            for (old_id, new_id) in outcome.id_map {
                if let Some(text) = sub.contents.remove(&old_id) {
                    if let Some(c) = counter
                        && let Some(node) = tree.get(new_id)
                        && let NodeKind::TextFile(file) = &node.kind
                    {
                        let delta =
                            compute_selected_xml_delta(c, &node.rel_path, &text, file.tokens);
                        xml_deltas.insert(new_id, delta);
                    }
                    contents.insert(new_id, text);
                }
            }
        }
    }
}
