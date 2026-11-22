//! Shared scanning, token-counting, secret-scanning, and initial TUI state assembly helpers.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::Path;

use crate::application::error::AppError;
use crate::config::limits::DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD;
use crate::domain::{DirectoryTree, GitContextSnapshot, NodeId, NodeKind};
use crate::output::{
    render_git_context_xml, render_selected_file_xml, render_xml_structure_envelope,
};
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

pub(super) fn compute_xml_file_tokens<S: BuildHasher>(
    tree: &DirectoryTree,
    contents: &HashMap<NodeId, String, S>,
) -> Result<HashMap<NodeId, usize>, AppError> {
    let counter = TokenCounter::o200k().map_err(AppError::Tokens)?;
    let mut rendered = Vec::new();
    for (idx, node) in tree.nodes().iter().enumerate() {
        if matches!(node.kind, NodeKind::TextFile(_)) {
            let id = NodeId(idx);
            if let Some(body) = contents.get(&id) {
                let mut buf = String::with_capacity(body.len().saturating_add(64));
                buf.push('\n');
                render_selected_file_xml(&node.rel_path, body, &mut buf);
                if buf.ends_with('\n') {
                    buf.pop();
                }
                rendered.push((id, buf));
            }
        }
    }

    let work: Vec<(NodeId, &str)> = rendered
        .iter()
        .map(|(id, text)| (*id, text.as_str()))
        .collect();
    Ok(counter.count_batch(&work).into_iter().collect())
}

pub(super) fn compute_xml_overhead_tokens(
    tree: &DirectoryTree,
    abs_root_path: Option<&str>,
    xml_file_tokens: &HashMap<NodeId, usize>,
) -> Result<usize, AppError> {
    let counter = TokenCounter::o200k().map_err(AppError::Tokens)?;
    let envelope = render_xml_structure_envelope(tree, abs_root_path);
    let envelope_tokens = counter.count(&envelope);

    let mut positive_extra = 0usize;
    let mut negative_extra = 0usize;
    for (idx, node) in tree.nodes().iter().enumerate() {
        if let NodeKind::TextFile(file) = &node.kind
            && file.state.is_selected()
            && let Some(&xml_tok) = xml_file_tokens.get(&NodeId(idx))
        {
            if xml_tok >= file.tokens {
                positive_extra = positive_extra.saturating_add(xml_tok - file.tokens);
            } else {
                negative_extra = negative_extra.saturating_add(file.tokens - xml_tok);
            }
        }
    }

    Ok(envelope_tokens
        .saturating_add(positive_extra)
        .saturating_sub(negative_extra))
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
