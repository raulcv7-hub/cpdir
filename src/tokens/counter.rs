//! Parallel `o200k_base` token counter decoupled from tree structures.

use std::fmt;
use std::sync::OnceLock;

use rayon::prelude::*;
use tiktoken_rs::CoreBPE;

use crate::config::limits::{TOKEN_CHUNK_TARGET_BYTES, TOKEN_CHUNK_THRESHOLD_BYTES};

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
    ///
    /// Inputs above [`TOKEN_CHUNK_THRESHOLD_BYTES`] are split along UTF-8 newline boundaries into
    /// bounded parallel slices so single giant headers or minified assets never stall or spike RAM.
    #[must_use]
    pub fn count(&self, text: &str) -> usize {
        if text.len() <= TOKEN_CHUNK_THRESHOLD_BYTES {
            return self.bpe.encode_ordinary(text).len();
        }
        let chunks = split_token_chunks(text, TOKEN_CHUNK_TARGET_BYTES);
        chunks
            .par_iter()
            .map(|&chunk| self.bpe.encode_ordinary(chunk).len())
            .sum()
    }

    /// Counts tokens for `(key, text)` entries sequentially or in parallel via `rayon`.
    #[must_use]
    pub fn count_batch<K: Copy + Send + Sync>(&self, items: &[(K, &str)]) -> Vec<(K, usize)> {
        if items.len() < PARALLEL_MIN_FILES {
            items
                .iter()
                .map(|&(key, text)| (key, self.count(text)))
                .collect()
        } else {
            items
                .par_iter()
                .map(|&(key, text)| (key, self.count(text)))
                .collect()
        }
    }
}

fn split_token_chunks(text: &str, target_bytes: usize) -> Vec<&str> {
    let total = text.len();
    let step = target_bytes.max(64);
    let mut chunks = Vec::with_capacity(total / step + 1);
    let mut start = 0usize;

    while start < total {
        let mut end = start.saturating_add(step).min(total);
        if end < total {
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            if let Some(nl_offset) = text[start..end].rfind('\n')
                && nl_offset >= step / 2
            {
                end = start + nl_offset + 1;
            }
        }
        if end == start {
            end = start
                + text[start..]
                    .chars()
                    .next()
                    .map_or(total - start, char::len_utf8);
        }
        chunks.push(&text[start..end]);
        start = end;
    }
    chunks
}
