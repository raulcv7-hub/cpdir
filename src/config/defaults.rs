//! Hard safety limits, pruned dependency directories, and binary extension defaults.

use std::path::Path;

/// Default token threshold above which readable files start deselected in the TUI.
pub const DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD: usize = 50_000;

/// Hard file size ceiling (`5 MiB`) preventing OOM or stalls on giant dumps and blobs.
pub const HARD_READ_BYTE_CAP: u64 = 5 * 1024 * 1024;

/// Directory names pruned completely before populating the TUI tree or XML output.
pub const PRUNED_DIR_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    "vendor",
    "__pycache__",
    ".venv",
    "venv",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".turbo",
    ".parcel-cache",
    ".gradle",
    ".idea",
    ".vs",
    "coverage",
    "obj",
];

/// Lowercase file extensions immediately classified as non-textual binaries.
pub const KNOWN_BINARY_EXTENSIONS: &[&str] = &[
    "7z", "a", "avi", "bin", "bmp", "bz2", "cab", "class", "cur", "dat", "db", "dll", "dylib",
    "eot", "exe", "flac", "gif", "gz", "ico", "iso", "jar", "jpeg", "jpg", "lib", "lnk", "lockb",
    "mkv", "mov", "mp3", "mp4", "msi", "o", "obj", "ogg", "otf", "pdb", "pdf", "png", "pyc", "pyo",
    "rar", "so", "sqlite", "tar", "tiff", "ttf", "wasm", "wav", "webm", "webp", "woff", "woff2",
    "xz", "zip", "zst",
];

/// Returns `true` when `dir_name` matches a dependency, build, or VCS folder to prune.
#[must_use]
pub fn is_pruned_dir_name(dir_name: &str) -> bool {
    PRUNED_DIR_NAMES
        .iter()
        .any(|candidate| dir_name.eq_ignore_ascii_case(candidate))
}

/// Returns `true` when `path` ends with a known binary file extension.
#[must_use]
pub fn is_known_binary_extension(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    KNOWN_BINARY_EXTENSIONS
        .iter()
        .any(|candidate| ext.eq_ignore_ascii_case(candidate))
}
