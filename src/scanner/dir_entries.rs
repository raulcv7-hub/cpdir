//! Sorted non-symlink directory entry enumeration and relative path resolution.

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

use crate::scanner::walker::ScanError;

pub(super) struct SortedEntry {
    pub(super) name: String,
    lower_name: String,
    pub(super) abs_path: PathBuf,
    pub(super) is_dir: bool,
}

pub(crate) fn resolve_rel_path(root: &Path, rel_path: &str) -> PathBuf {
    rel_path
        .split('/')
        .filter(|seg| !seg.is_empty())
        .fold(root.to_path_buf(), |acc, seg| acc.join(seg))
}

pub(super) fn read_sorted_entries(dir: &Path) -> Result<Vec<SortedEntry>, ScanError> {
    let read_dir = fs::read_dir(dir).map_err(|err| ScanError::ReadDirFailed {
        path: dir.to_path_buf(),
        reason: err.to_string(),
    })?;

    let mut entries = Vec::new();
    for item_res in read_dir {
        let Ok(item) = item_res else {
            continue;
        };
        // Never follow NTFS junctions or symlinks to prevent directory loops and escapes.
        let Ok(ft) = item.file_type() else {
            continue;
        };
        if ft.is_symlink() || (!ft.is_dir() && !ft.is_file()) {
            continue;
        }
        let name = item.file_name().to_string_lossy().into_owned();
        if name.is_empty() {
            continue;
        }
        entries.push(SortedEntry {
            lower_name: name.to_ascii_lowercase(),
            name,
            abs_path: item.path(),
            is_dir: ft.is_dir(),
        });
    }

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a
            .lower_name
            .cmp(&b.lower_name)
            .then_with(|| a.name.cmp(&b.name)),
    });

    Ok(entries)
}
