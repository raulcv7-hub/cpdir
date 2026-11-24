//! Parallel streaming file ingestion combining 8 KiB probing, secret scanning, and single-pass BPE.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use rayon::prelude::*;

use crate::application::analyze::{compute_selected_xml_delta, is_cancelled_flag};
use crate::application::error::AppError;
use crate::config::limits::{
    DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD, INLINE_CONTENT_CACHE_BYTES,
    MAX_SELECTED_INLINE_CACHE_BYTES, MAX_UNSELECTED_INLINE_CACHE_BYTES,
};
use crate::domain::{
    DirectoryTree, FileDiscardReason, NodeId, NodeKind, SecretFinding, TextFileState,
};
use crate::scanner::{
    DiscoveredFile, ReadFileOutcome, ScanConfig, discover_workspace_entries, read_bounded_utf8,
};
use crate::security::SecretScanner;
use crate::tokens::TokenCounter;
use crate::tui::{LoadingPhase, LoadingProgress};

const INGEST_BATCH_SIZE: usize = 64;

pub(super) struct StreamingIngestOutput {
    pub(super) tree: DirectoryTree,
    pub(super) contents: HashMap<NodeId, String>,
    pub(super) xml_file_deltas: HashMap<NodeId, isize>,
    pub(super) cancelled: bool,
}

enum FileEval {
    Discarded {
        id: NodeId,
        reason: FileDiscardReason,
    },
    Text {
        id: NodeId,
        byte_len: u64,
        tokens: usize,
        findings: Vec<SecretFinding>,
        exceeds_threshold: bool,
        selected: bool,
        content: Option<String>,
        xml_delta: Option<isize>,
    },
}

pub(super) fn scan_and_ingest_streaming(
    target_dir: &Path,
    config: &ScanConfig,
    cancel: Option<&AtomicBool>,
    progress: &mut LoadingProgress,
    on_progress: &mut dyn FnMut(&LoadingProgress),
) -> Result<StreamingIngestOutput, AppError> {
    let mut disc = discover_workspace_entries(target_dir, config, cancel, &mut |dp| {
        progress.dirs_scanned = dp.dirs_scanned;
        progress.pruned_dirs = dp.pruned_dirs;
        progress.candidate_files = dp.candidate_files;
        progress.discarded_files = dp.discarded_files;
        on_progress(progress);
    })
    .map_err(AppError::Scan)?;

    if disc.cancelled || is_cancelled_flag(cancel) {
        return Ok(StreamingIngestOutput {
            tree: disc.tree,
            contents: HashMap::new(),
            xml_file_deltas: HashMap::new(),
            cancelled: true,
        });
    }

    progress.phase = LoadingPhase::Analyzing;
    on_progress(progress);

    let counter = TokenCounter::o200k().map_err(AppError::Tokens)?;
    let scanner = SecretScanner::global();
    let mut contents = HashMap::new();
    let mut xml_file_deltas = HashMap::new();
    let mut selected_cached_bytes = 0u64;
    let mut unselected_cached_bytes = 0u64;

    for chunk in disc.files.chunks(INGEST_BATCH_SIZE) {
        if is_cancelled_flag(cancel) {
            return Ok(StreamingIngestOutput {
                tree: disc.tree,
                contents,
                xml_file_deltas,
                cancelled: true,
            });
        }

        let evals: Vec<FileEval> = chunk
            .par_iter()
            .map(|item| evaluate_discovered_file(item, config.max_file_bytes, counter, scanner))
            .collect();

        apply_eval_batch(
            &mut disc.tree,
            evals,
            &mut contents,
            &mut xml_file_deltas,
            &mut selected_cached_bytes,
            &mut unselected_cached_bytes,
            progress,
        );
        on_progress(progress);
    }

    Ok(StreamingIngestOutput {
        tree: disc.tree,
        contents,
        xml_file_deltas,
        cancelled: false,
    })
}

fn evaluate_discovered_file(
    item: &DiscoveredFile,
    max_file_bytes: u64,
    counter: TokenCounter,
    scanner: &SecretScanner,
) -> FileEval {
    let (byte_len, content) = match read_bounded_utf8(&item.abs_path, max_file_bytes) {
        ReadFileOutcome::Text { byte_len, content } => (byte_len, content),
        ReadFileOutcome::Discarded(reason) => {
            return FileEval::Discarded {
                id: item.node_id,
                reason,
            };
        }
    };

    let findings = scanner.scan_text(&item.rel_path, &content);
    let tokens = counter.count(&content);

    let mut effective_state = TextFileState::new(item.natural);
    if item.mark_pruned {
        effective_state.mark_pruned();
    }
    let exceeds_threshold = tokens > DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD;
    if exceeds_threshold {
        effective_state.mark_exceeds_token_threshold();
    }

    let selected = effective_state.is_selected();
    let xml_delta =
        selected.then(|| compute_selected_xml_delta(counter, &item.rel_path, &content, tokens));

    // Evict large unselected payloads immediately inside workers so peak RSS stays bounded.
    let kept = if !selected && byte_len > INLINE_CONTENT_CACHE_BYTES {
        None
    } else {
        Some(content)
    };

    FileEval::Text {
        id: item.node_id,
        byte_len,
        tokens,
        findings,
        exceeds_threshold,
        selected,
        content: kept,
        xml_delta,
    }
}

fn apply_eval_batch(
    tree: &mut DirectoryTree,
    evals: Vec<FileEval>,
    contents: &mut HashMap<NodeId, String>,
    xml_file_deltas: &mut HashMap<NodeId, isize>,
    selected_cached_bytes: &mut u64,
    unselected_cached_bytes: &mut u64,
    progress: &mut LoadingProgress,
) {
    for eval in evals {
        progress.analyzed_files += 1;
        match eval {
            FileEval::Discarded { id, reason } => {
                if let Some(node) = tree.get_mut(id) {
                    node.kind = NodeKind::DiscardedFile(reason);
                }
                progress.discarded_files += 1;
            }
            FileEval::Text {
                id,
                byte_len,
                tokens,
                findings,
                exceeds_threshold,
                selected,
                content,
                xml_delta,
            } => {
                if let Some(node) = tree.get_mut(id)
                    && let NodeKind::TextFile(file) = &mut node.kind
                {
                    file.byte_len = byte_len;
                    file.tokens = tokens;
                    if exceeds_threshold {
                        file.state.mark_exceeds_token_threshold();
                    }
                }
                tree.set_secret_findings(id, findings);
                if selected {
                    progress.selected_tokens = progress.selected_tokens.saturating_add(tokens);
                }
                if let Some(delta) = xml_delta {
                    xml_file_deltas.insert(id, delta);
                }
                if let Some(text) = content {
                    cache_bounded_content(
                        id,
                        byte_len,
                        selected,
                        text,
                        contents,
                        selected_cached_bytes,
                        unselected_cached_bytes,
                    );
                }
            }
        }
    }
}

fn cache_bounded_content(
    id: NodeId,
    byte_len: u64,
    selected: bool,
    text: String,
    contents: &mut HashMap<NodeId, String>,
    selected_cached_bytes: &mut u64,
    unselected_cached_bytes: &mut u64,
) {
    if selected {
        if selected_cached_bytes.saturating_add(byte_len) <= MAX_SELECTED_INLINE_CACHE_BYTES {
            *selected_cached_bytes = selected_cached_bytes.saturating_add(byte_len);
            contents.insert(id, text);
        }
    } else if byte_len <= INLINE_CONTENT_CACHE_BYTES
        && unselected_cached_bytes.saturating_add(byte_len) <= MAX_UNSELECTED_INLINE_CACHE_BYTES
    {
        *unselected_cached_bytes = unselected_cached_bytes.saturating_add(byte_len);
        contents.insert(id, text);
    }
}
