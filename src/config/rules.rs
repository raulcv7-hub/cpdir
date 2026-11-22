//! Classification tables for directory pruning, binary extensions, and secret-scan exemptions.

use std::path::Path;

/// Directory names pruned unconditionally at any depth (VCS, caches, and package dependencies).
pub const UNCONDITIONAL_PRUNED_DIR_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
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
    ".angular",
    ".expo",
    ".gradle",
    ".idea",
    ".vs",
    ".terraform",
    ".dart_tool",
    ".tox",
    ".nox",
];

/// Directory names pruned when outside explicit source-code subtrees (`src/`, `lib/`, etc.).
pub const CONTEXT_PRUNED_DIR_NAMES: &[&str] =
    &["build", "dist", "vendor", "obj", "coverage", "out"];

const SOURCE_ANCESTOR_DIRS: &[&str] = &["src", "source", "sources", "lib", "internal", "cmd"];

/// Lowercase file extensions immediately classified as non-textual binaries.
pub const KNOWN_BINARY_EXTENSIONS: &[&str] = &[
    "7z", "a", "avi", "bin", "bmp", "bz2", "cab", "class", "cur", "dat", "db", "dll", "dylib",
    "eot", "exe", "flac", "gif", "gz", "ico", "iso", "jar", "jpeg", "jpg", "lib", "lnk", "lockb",
    "mkv", "mov", "mp3", "mp4", "msi", "o", "obj", "ogg", "otf", "pdb", "pdf", "png", "pyc", "pyo",
    "rar", "so", "sqlite", "tar", "tiff", "ttf", "wasm", "wav", "webm", "webp", "woff", "woff2",
    "xz", "zip", "zst",
];

/// Template and lockfile basenames exempted from heuristic secret scanning alerts.
pub const SECRET_EXEMPT_FILENAMES: &[&str] = &[
    ".env.example",
    ".env.sample",
    ".env.template",
    ".env.dist",
    ".env.defaults",
    "env.example",
    "example.env",
    "sample.env",
    "template.env",
    "Cargo.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "uv.lock",
    "poetry.lock",
];

/// Returns `true` when `dir_name` matches a known dependency, build, cache, or VCS folder.
#[must_use]
pub fn is_pruned_dir_name(dir_name: &str) -> bool {
    matches_any_ignore_case(UNCONDITIONAL_PRUNED_DIR_NAMES, dir_name)
        || matches_any_ignore_case(CONTEXT_PRUNED_DIR_NAMES, dir_name)
}

/// Decides whether a directory at `rel_path` should be pruned before tree ingestion.
///
/// Unconditional VCS/cache/dependency directories prune at every depth, whereas ambiguous output
/// names (`build`, `dist`, `vendor`, `obj`, `coverage`, `out`) remain intact inside source trees.
#[must_use]
pub fn should_prune_dir(rel_path: &str, dir_name: &str) -> bool {
    if matches_any_ignore_case(UNCONDITIONAL_PRUNED_DIR_NAMES, dir_name) {
        return true;
    }
    matches_any_ignore_case(CONTEXT_PRUNED_DIR_NAMES, dir_name) && !has_source_ancestor(rel_path)
}

/// Returns `true` when `path` ends with a known binary file extension.
#[must_use]
pub fn is_known_binary_extension(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches_any_ignore_case(KNOWN_BINARY_EXTENSIONS, ext)
}

/// Returns `true` when `rel_path` refers to an exempt template, `.env.*.example`, or lockfile.
#[must_use]
pub fn is_secret_scan_exempt_path(rel_path: &str) -> bool {
    let Some(file_name) = Path::new(rel_path).file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if matches_any_ignore_case(SECRET_EXEMPT_FILENAMES, file_name) {
        return true;
    }

    let lower = file_name.to_ascii_lowercase();
    let template_suffixes = [".example", ".sample", ".template", ".dist", ".defaults"];
    if (lower.starts_with(".env.") || lower.starts_with("env."))
        && template_suffixes.iter().any(|sfx| lower.ends_with(sfx))
    {
        return true;
    }

    [".example.", ".sample.", ".template."]
        .iter()
        .any(|marker| lower.contains(marker))
}

fn matches_any_ignore_case(candidates: &[&str], value: &str) -> bool {
    candidates
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

fn has_source_ancestor(rel_path: &str) -> bool {
    let Some((parent, _)) = rel_path.rsplit_once('/') else {
        return false;
    };
    parent
        .split('/')
        .any(|segment| matches_any_ignore_case(SOURCE_ANCESTOR_DIRS, segment))
}
