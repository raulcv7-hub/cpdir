//! Real temporary workspace builder for ingestion, binary, and encoding fixtures.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// Isolated temporary workspace directory cleaned up automatically on drop.
pub(crate) struct WorkspaceBuilder {
    temp_dir: TempDir,
}

impl WorkspaceBuilder {
    /// Creates a new empty temporary workspace directory on disk.
    pub(crate) fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp workspace directory");
        Self { temp_dir }
    }

    /// Returns the absolute root path of the temporary workspace.
    #[must_use]
    pub(crate) fn root(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Writes UTF-8 text to a relative path inside the workspace, creating parent folders.
    pub(crate) fn write_text(&self, rel_path: &str, content: &str) -> PathBuf {
        self.write_bytes(rel_path, content.as_bytes())
    }

    /// Writes raw bytes to a relative path inside the workspace, creating parent folders.
    pub(crate) fn write_bytes(&self, rel_path: &str, bytes: &[u8]) -> PathBuf {
        let full_path = self.root().join(rel_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).expect("failed to create fixture parent directory");
        }
        fs::write(&full_path, bytes).expect("failed to write fixture file");
        full_path
    }

    /// Creates an explicit directory (including empty directories) at the relative path.
    pub(crate) fn create_dir(&self, rel_path: &str) -> PathBuf {
        let full_path = self.root().join(rel_path);
        fs::create_dir_all(&full_path).expect("failed to create fixture directory");
        full_path
    }

    /// Populates a subdirectory with `count` synthetic files to verify dependency pruning.
    pub(crate) fn populate_files(&self, rel_dir: &str, count: usize, prefix: &str) {
        for idx in 0..count {
            let rel_path = format!("{rel_dir}/{prefix}_{idx}.js");
            let body = format!("export const id = {idx};\n");
            self.write_text(&rel_path, &body);
        }
    }
}

/// Returns a PNG file header followed by binary bytes for disguised-binary fixtures.
#[must_use]
pub(crate) fn sample_png_bytes() -> Vec<u8> {
    vec![
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
    ]
}

/// Returns valid UTF-8 text corrupted mid-stream by invalid continuation bytes.
#[must_use]
pub(crate) fn mid_stream_corrupt_utf8_bytes() -> Vec<u8> {
    b"pub fn ok() {}\n\xFF\xFE\x80invalid\n".to_vec()
}

/// Encodes `text` as UTF-16 Little Endian prefixed with a BOM.
#[must_use]
pub(crate) fn utf16_le_with_bom_bytes(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}
