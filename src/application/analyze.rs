//! Shared token-counting and secret-scanning orchestration over ingested trees.

use std::collections::HashMap;
use std::hash::BuildHasher;

use crate::application::error::AppError;
use crate::config::limits::DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD;
use crate::domain::{DirectoryTree, NodeId};
use crate::security::{SecretScanner, populate_secret_hits};
use crate::tokens::{TokenCounter, populate_token_counts};

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
