//! Shared scanning, token-counting, secret-scanning, and initial TUI state assembly helpers.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::Path;

use crate::application::error::AppError;
use crate::config::limits::DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD;
use crate::domain::{DirectoryTree, GitContextSnapshot, NodeId};
use crate::output::render_git_context_xml;
use crate::scanner::{ScanConfig, ScanResult, scan_directory};
use crate::security::{SecretScanner, populate_secret_hits};
use crate::state::SavedWorkspaceState;
use crate::tokens::{TokenCounter, populate_token_counts};
use crate::tui::TuiAppState;

/// Populates `o200k_base` token counts and heuristic secret hits across `tree`.
///
/// # Errors
///
/// Returns [`AppError::Tokens`] if the embedded `o200k_base` tokenizer fails to initialize.
pub fn analyze_tree_contents<S: BuildHasher>(
    tree: &mut DirectoryTree,
    contents: &HashMap<NodeId, String, S>,
) -> Result<(), AppError> {
    populate_secret_hits(tree, contents, SecretScanner::global());
    let counter = TokenCounter::o200k().map_err(AppError::Tokens)?;
    populate_token_counts(
        tree,
        contents,
        DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD,
        counter,
    );
    Ok(())
}

pub(super) fn scan_and_analyze(
    target_dir: &Path,
    config: &ScanConfig,
) -> Result<ScanResult, AppError> {
    let mut scan = scan_directory(target_dir, config).map_err(AppError::Scan)?;
    analyze_tree_contents(&mut scan.tree, &scan.contents)?;
    Ok(scan)
}

pub(super) fn build_initial_tui_state(
    tree: DirectoryTree,
    git_snapshot: Option<&GitContextSnapshot>,
    saved: Option<&SavedWorkspaceState>,
) -> TuiAppState {
    let mut state = TuiAppState::new(tree, git_snapshot.is_some());
    if let Some(snap) = git_snapshot
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
    state
}
