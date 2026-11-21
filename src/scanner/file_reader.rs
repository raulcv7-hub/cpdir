//! Bounded `UTF-8` reader with fast binary probes and `BOM` handling.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::config::defaults::is_known_binary_extension;
use crate::domain::FileDiscardReason;

/// Classification returned after attempting to ingest a file from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadFileOutcome {
    /// Valid `UTF-8` text ready for tokenization and secret scanning.
    Text {
        /// Original on-disk byte length prior to stripping a leading `UTF-8` `BOM`.
        byte_len: u64,
        /// Valid `UTF-8` text payload without a leading `UTF-8` `BOM`.
        content: String,
    },
    /// File cannot enter `<files>` due to binary data, invalid `UTF-8`, size cap, or I/O error.
    Discarded(FileDiscardReason),
}

/// Reads `path` up to `max_bytes`, rejecting binaries, non-`UTF-8` encodings, and huge files.
#[must_use]
pub fn read_bounded_utf8(path: &Path, max_bytes: u64) -> ReadFileOutcome {
    if is_known_binary_extension(path) {
        return ReadFileOutcome::Discarded(FileDiscardReason::Binary);
    }

    let Ok(file) = File::open(path) else {
        return ReadFileOutcome::Discarded(FileDiscardReason::Unreadable);
    };
    let Ok(meta) = file.metadata() else {
        return ReadFileOutcome::Discarded(FileDiscardReason::Unreadable);
    };
    if !meta.is_file() {
        return ReadFileOutcome::Discarded(FileDiscardReason::Unreadable);
    }
    if meta.len() > max_bytes {
        return ReadFileOutcome::Discarded(FileDiscardReason::ExceedsHardCap {
            byte_len: meta.len(),
        });
    }

    let cap_hint = usize::try_from(meta.len().min(max_bytes)).unwrap_or(0);
    let mut buf = Vec::with_capacity(cap_hint);
    if file
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut buf)
        .is_err()
    {
        return ReadFileOutcome::Discarded(FileDiscardReason::Unreadable);
    }

    let byte_len = u64::try_from(buf.len()).unwrap_or(u64::MAX);
    if byte_len > max_bytes {
        return ReadFileOutcome::Discarded(FileDiscardReason::ExceedsHardCap { byte_len });
    }
    if starts_with_non_utf8_bom(&buf) {
        return ReadFileOutcome::Discarded(FileDiscardReason::InvalidUtf8);
    }
    if buf.contains(&0) {
        return ReadFileOutcome::Discarded(FileDiscardReason::Binary);
    }

    let Ok(mut content) = String::from_utf8(buf) else {
        return ReadFileOutcome::Discarded(FileDiscardReason::InvalidUtf8);
    };
    if content.starts_with('\u{FEFF}') {
        content.remove(0);
    }

    ReadFileOutcome::Text { byte_len, content }
}

const fn starts_with_non_utf8_bom(bytes: &[u8]) -> bool {
    matches!(
        bytes,
        [0xFF, 0xFE, ..] | [0xFE, 0xFF, ..] | [0x00, 0x00, 0xFE, 0xFF, ..]
    )
}
