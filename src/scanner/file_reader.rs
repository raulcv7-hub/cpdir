//! Bounded `UTF-8` reader with fast binary probes, cloud-placeholder guard, and `BOM` handling.

use std::fs::File;
use std::io::Read;
#[cfg(windows)]
use std::os::windows::fs::MetadataExt;
use std::path::Path;

use crate::config::defaults::is_known_binary_extension;
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

/// Reads `path` up to `max_bytes`, rejecting binaries, cloud placeholders, invalid `UTF-8`, and huge files.
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

#[cfg(windows)]
fn is_cloud_placeholder(path: &Path) -> bool {
    // Inspect NTFS reparse/offline attributes via metadata before File::open triggers hydration.
    std::fs::symlink_metadata(path)
        .is_ok_and(|meta| is_cloud_placeholder_attributes(meta.file_attributes()))
}

#[cfg(not(windows))]
const fn is_cloud_placeholder(_path: &Path) -> bool {
    false
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
