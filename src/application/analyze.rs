//! Shared token-counting, secret-scanning, XML delta sizing, and initial TUI state assembly.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::application::error::AppError;
use crate::config::limits::{
    DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD, INLINE_XML_DELTA_BODY_CAP_BYTES,
};
use crate::domain::{DirectoryTree, GitContextSnapshot, NodeId, NodeKind};
use crate::output::{
    render_git_context_xml, render_selected_file_xml, render_xml_structure_envelope,
};
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

pub(super) fn is_cancelled_flag(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|flag| flag.load(Ordering::Acquire))
}

pub(super) fn compute_selected_xml_delta(
    counter: TokenCounter,
    rel_path: &str,
    body: &str,
    raw_tokens: usize,
) -> isize {
    if body.len() <= INLINE_XML_DELTA_BODY_CAP_BYTES {
        let mut buf = String::with_capacity(body.len().saturating_add(64));
        buf.push('\n');
        render_selected_file_xml(rel_path, body, &mut buf);
        if buf.ends_with('\n') {
            buf.pop();
        }
        let xml_tok = counter.count(&buf);
        return signed_diff(xml_tok, raw_tokens);
    }

    // Avoid re-tokenizing multi-megabyte bodies just to account for XML file tags.
    let wrapper_tok = count_wrapper_only_tokens(counter, rel_path);
    isize::try_from(wrapper_tok).unwrap_or(isize::MAX)
}

pub(super) fn compute_xml_overhead_tokens(
    tree: &DirectoryTree,
    abs_root_path: Option<&str>,
    xml_file_deltas: &HashMap<NodeId, isize>,
) -> Result<usize, AppError> {
    let counter = TokenCounter::o200k().map_err(AppError::Tokens)?;
    let envelope = render_xml_structure_envelope(tree, abs_root_path);
    let envelope_tokens = counter.count(&envelope);

    let mut net_delta: isize = 0;
    for (idx, node) in tree.nodes().iter().enumerate() {
        if let NodeKind::TextFile(file) = &node.kind
            && file.state.is_selected()
        {
            let delta = xml_file_deltas
                .get(&NodeId(idx))
                .copied()
                .unwrap_or_else(|| {
                    let w = if file.byte_len == 0 {
                        count_empty_tag_tokens(counter, &node.rel_path)
                    } else {
                        count_wrapper_only_tokens(counter, &node.rel_path)
                    };
                    isize::try_from(w).unwrap_or(0)
                });
            net_delta = net_delta.saturating_add(delta);
        }
    }

    let base = isize::try_from(envelope_tokens).unwrap_or(isize::MAX);
    Ok(usize::try_from(base.saturating_add(net_delta).max(0)).unwrap_or(0))
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

fn count_empty_tag_tokens(counter: TokenCounter, rel_path: &str) -> usize {
    let mut buf = String::with_capacity(rel_path.len() + 32);
    buf.push('\n');
    render_selected_file_xml(rel_path, "", &mut buf);
    if buf.ends_with('\n') {
        buf.pop();
    }
    counter.count(&buf)
}

fn count_wrapper_only_tokens(counter: TokenCounter, rel_path: &str) -> usize {
    let mut buf = String::with_capacity(rel_path.len() + 48);
    buf.push('\n');
    render_selected_file_xml(rel_path, " ", &mut buf);
    if buf.ends_with('\n') {
        buf.pop();
    }
    if let Some((open, close_rest)) = buf.split_once(" ]]>") {
        let open_with_space = format!("{open} ");
        let close = format!("]]>{close_rest}");
        counter.count(&open_with_space) + counter.count(&close)
    } else {
        counter.count(&buf)
    }
}

fn signed_diff(lhs: usize, rhs: usize) -> isize {
    if lhs >= rhs {
        isize::try_from(lhs - rhs).unwrap_or(isize::MAX)
    } else {
        -isize::try_from(rhs - lhs).unwrap_or(isize::MAX)
    }
}
