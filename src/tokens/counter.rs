//! Parallel `o200k_base` token counter using `tiktoken-rs` and `rayon`.

use std::collections::HashMap;
use std::fmt;
use std::hash::BuildHasher;
use std::sync::OnceLock;

use rayon::prelude::*;
use tiktoken_rs::CoreBPE;

use crate::domain::{DirectoryTree, NodeId, NodeKind};

const PARALLEL_MIN_FILES: usize = 8;

static O200K_BPE: OnceLock<Result<CoreBPE, String>> = OnceLock::new();

/// Errors emitted while initializing the offline BPE tokenizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
    /// Failed to initialize the embedded `o200k_base` BPE dictionary.
    InitFailed(String),
}

impl fmt::Display for TokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InitFailed(reason) => write!(f, "failed to initialize tokenizer: {reason}"),
        }
    }
}

impl std::error::Error for TokenError {}

/// Thread-safe handle to the process-wide `o200k_base` BPE tokenizer.
#[derive(Clone, Copy)]
pub struct TokenCounter {
    bpe: &'static CoreBPE,
}

impl fmt::Debug for TokenCounter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenCounter")
            .field("encoding", &"o200k_base")
            .finish()
    }
}

impl TokenCounter {
    /// Returns a handle to the shared `o200k_base` tokenizer, initializing it once on first call.
    ///
    /// # Errors
    ///
    /// Returns [`TokenError::InitFailed`] if the embedded BPE dictionary fails to load.
    pub fn o200k() -> Result<Self, TokenError> {
        let cached = O200K_BPE.get_or_init(|| tiktoken_rs::o200k_base().map_err(|e| e.to_string()));
        match cached {
            Ok(bpe) => Ok(Self { bpe }),
            Err(msg) => Err(TokenError::InitFailed(msg.clone())),
        }
    }

    /// Counts ordinary BPE tokens in `text` without failing on special LLM control markers.
    #[must_use]
    pub fn count(&self, text: &str) -> usize {
        self.bpe.encode_ordinary(text).len()
    }
}

/// Counts tokens for all readable text files in `tree` and auto-deselects entries above `threshold`.
pub fn populate_token_counts<S: BuildHasher>(
    tree: &mut DirectoryTree,
    contents: &HashMap<NodeId, String, S>,
    auto_deselect_threshold: usize,
    counter: TokenCounter,
) {
    let work: Vec<(NodeId, &str)> = tree
        .nodes()
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| {
            if matches!(node.kind, NodeKind::TextFile(_)) {
                let id = NodeId(idx);
                contents.get(&id).map(|text| (id, text.as_str()))
            } else {
                None
            }
        })
        .collect();

    let counts: Vec<(NodeId, usize)> = if work.len() < PARALLEL_MIN_FILES {
        work.into_iter()
            .map(|(id, text)| (id, counter.count(text)))
            .collect()
    } else {
        work.par_iter()
            .map(|&(id, text)| (id, counter.count(text)))
            .collect()
    };

    for (id, token_count) in counts {
        if let Some(node) = tree.get_mut(id)
            && let NodeKind::TextFile(file) = &mut node.kind
        {
            file.tokens = token_count;
            if token_count > auto_deselect_threshold {
                file.state.mark_exceeds_token_threshold();
            }
        }
    }
}
