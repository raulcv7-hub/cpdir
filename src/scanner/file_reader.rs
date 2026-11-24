//! Bounded `UTF-8` reader with `8 KiB` binary probing, cloud-placeholder guard, and `BOM` handling.

use std::fs::File;
use std::io::Read;
#[cfg(windows)]
use std::os::windows::fs::MetadataExt;
use std::path::Path;

use crate::config::limits::FILE_HEADER_PROBE_BYTES;
use crate::config::rules::is_known_binary_extension;
use crate::domain::FileDiscardReason;

#[cfg(any(windows, test))]
const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
#[cfg(any(windows, test))]
const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
#[cfg(any(windows, test))]
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

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

enum ProbeStage {
    Complete(Vec<u8>),
    NeedsRemainder(Vec<u8>),
    Discarded(FileDiscardReason),
}

/// Reads `path` up to `max_bytes`, rejecting binaries via an `8 KiB` probe before full buffering.
#[must_use]
pub fn read_bounded_utf8(path: &Path, max_bytes: u64) -> ReadFileOutcome {
    if is_known_binary_extension(path) {
        return ReadFileOutcome::Discarded(FileDiscardReason::Binary);
    }
    if is_cloud_placeholder(path) {
        return ReadFileOutcome::Discarded(FileDiscardReason::Unreadable);
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

    let mut buf = match probe_header(&file, max_bytes, meta.len()) {
        ProbeStage::Complete(done) => return finalize_utf8(done),
        ProbeStage::NeedsRemainder(partial) => partial,
        ProbeStage::Discarded(reason) => return ReadFileOutcome::Discarded(reason),
    };

    let probe_len = buf.len();
    let remaining_allowed = max_bytes.saturating_sub(u64::try_from(probe_len).unwrap_or(u64::MAX));
    let extra_hint = usize::try_from(meta.len().min(max_bytes))
        .unwrap_or(0)
        .saturating_sub(probe_len);
    buf.reserve(extra_hint);

    if (&file)
        .take(remaining_allowed.saturating_add(1))
        .read_to_end(&mut buf)
        .is_err()
    {
        return ReadFileOutcome::Discarded(FileDiscardReason::Unreadable);
    }

    let byte_len = u64::try_from(buf.len()).unwrap_or(u64::MAX);
    if byte_len > max_bytes {
        return ReadFileOutcome::Discarded(FileDiscardReason::ExceedsHardCap { byte_len });
    }
    if buf[probe_len..].contains(&0) {
        return ReadFileOutcome::Discarded(FileDiscardReason::Binary);
    }

    finalize_utf8(buf)
}

#[cfg(windows)]
pub(crate) fn is_cloud_placeholder(path: &Path) -> bool {
    // Inspect NTFS reparse/offline attributes via metadata before File::open triggers hydration.
    std::fs::symlink_metadata(path)
        .is_ok_and(|meta| is_cloud_placeholder_attributes(meta.file_attributes()))
}

#[cfg(not(windows))]
pub(crate) const fn is_cloud_placeholder(_path: &Path) -> bool {
    false
}

fn probe_header(file: &File, max_bytes: u64, meta_len: u64) -> ProbeStage {
    let probe_limit = max_bytes.min(u64::try_from(FILE_HEADER_PROBE_BYTES).unwrap_or(u64::MAX));
    let probe_cap = usize::try_from(probe_limit).unwrap_or(FILE_HEADER_PROBE_BYTES);
    let hint = usize::try_from(meta_len.min(probe_limit)).unwrap_or(probe_cap);
    let mut buf = Vec::with_capacity(hint.saturating_add(1));

    if file
        .take(probe_limit.saturating_add(1))
        .read_to_end(&mut buf)
        .is_err()
    {
        return ProbeStage::Discarded(FileDiscardReason::Unreadable);
    }

    let byte_len = u64::try_from(buf.len()).unwrap_or(u64::MAX);
    if byte_len > max_bytes {
        return ProbeStage::Discarded(FileDiscardReason::ExceedsHardCap { byte_len });
    }
    if starts_with_non_utf8_bom(&buf) {
        return ProbeStage::Discarded(FileDiscardReason::InvalidUtf8);
    }
    if buf.contains(&0) {
        return ProbeStage::Discarded(FileDiscardReason::Binary);
    }

    let hit_eof = buf.len() <= probe_cap;
    if hit_eof {
        return ProbeStage::Complete(buf);
    }

    if let Err(err) = std::str::from_utf8(&buf)
        && err.error_len().is_some()
    {
        return ProbeStage::Discarded(FileDiscardReason::InvalidUtf8);
    }
    ProbeStage::NeedsRemainder(buf)
}

fn finalize_utf8(buf: Vec<u8>) -> ReadFileOutcome {
    let byte_len = u64::try_from(buf.len()).unwrap_or(u64::MAX);
    let Ok(mut content) = String::from_utf8(buf) else {
        return ReadFileOutcome::Discarded(FileDiscardReason::InvalidUtf8);
    };
    if content.starts_with('\u{FEFF}') {
        content.remove(0);
    }
    ReadFileOutcome::Text { byte_len, content }
}

#[cfg(any(windows, test))]
const fn is_cloud_placeholder_attributes(attrs: u32) -> bool {
    (attrs
        & (FILE_ATTRIBUTE_OFFLINE
            | FILE_ATTRIBUTE_RECALL_ON_OPEN
            | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS))
        != 0
}

const fn starts_with_non_utf8_bom(bytes: &[u8]) -> bool {
    matches!(
        bytes,
        [0xFF, 0xFE, ..] | [0xFE, 0xFF, ..] | [0x00, 0x00, 0xFE, 0xFF, ..]
    )
}

#[cfg(test)]
mod tests {
    use super::is_cloud_placeholder_attributes;

    #[test]
    fn identifies_onedrive_cloud_attributes_without_flagging_local_files() {
        assert!(!is_cloud_placeholder_attributes(0x0000_0020)); // FILE_ATTRIBUTE_ARCHIVE
        assert!(is_cloud_placeholder_attributes(0x0000_1000)); // FILE_ATTRIBUTE_OFFLINE
        assert!(is_cloud_placeholder_attributes(0x0004_0000)); // RECALL_ON_OPEN
        assert!(is_cloud_placeholder_attributes(0x0040_0000)); // RECALL_ON_DATA_ACCESS
    }
}
