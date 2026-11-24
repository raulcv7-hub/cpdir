//! Classification tables for directory pruning, binary extensions, and secret-scan exemptions.

use std::path::Path;

/// Directory names pruned unconditionally at any depth (VCS, caches, and build/package artifacts).
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
    "CMakeFiles",
    "cmake-build-debug",
    "cmake-build-release",
    "cmake-build-relwithdebinfo",
    "cmake-build-minsizerel",
    "vcpkg_installed",
    ".cache",
    ".ccache",
    "DerivedDataCache",
    "Intermediate",
    ".xmake",
    ".cxx",
    ".conan",
    ".conan2",
];

/// Directory names pruned when outside explicit source-code subtrees (`src/`, `lib/`, etc.).
pub const CONTEXT_PRUNED_DIR_NAMES: &[&str] =
    &["build", "dist", "vendor", "obj", "coverage", "out"];

const SOURCE_ANCESTOR_DIRS: &[&str] = &["src", "source", "sources", "lib", "internal", "cmd"];

/// Lowercase file extensions immediately classified as non-textual binaries.
pub const KNOWN_BINARY_EXTENSIONS: &[&str] = &[
    "3ds",
    "3gp",
    "7z",
    "a",
    "aab",
    "aac",
    "abc",
    "accdb",
    "aiff",
    "air",
    "aof",
    "apk",
    "appx",
    "arrow",
    "arw",
    "astc",
    "avi",
    "avif",
    "avro",
    "axf",
    "b3d",
    "basis",
    "bc",
    "bgeo",
    "bin",
    "blend",
    "blend1",
    "bmp",
    "br",
    "bsc",
    "bson",
    "bz2",
    "c4d",
    "cab",
    "cbor",
    "chm",
    "ckpt",
    "class",
    "cpio",
    "cpl",
    "cr2",
    "cso",
    "cubin",
    "cur",
    "dat",
    "db",
    "db-shm",
    "db-wal",
    "dds",
    "deb",
    "dll",
    "dmg",
    "dmp",
    "dng",
    "docx",
    "drv",
    "dxbc",
    "dxil",
    "dylib",
    "egg",
    "elf",
    "eot",
    "epub",
    "exe",
    "exp",
    "exr",
    "fatbin",
    "fbx",
    "feather",
    "fla",
    "flac",
    "flv",
    "gch",
    "ggml",
    "gguf",
    "gif",
    "glb",
    "gz",
    "h5",
    "hdf5",
    "hdmp",
    "hdr",
    "heic",
    "heif",
    "icns",
    "ico",
    "idb",
    "ilk",
    "img",
    "iobj",
    "ipa",
    "ipch",
    "ipdb",
    "iqm",
    "iso",
    "jar",
    "joblib",
    "jpeg",
    "jpg",
    "jxl",
    "ko",
    "ktx",
    "ktx2",
    "ldb",
    "lib",
    "lnk",
    "lockb",
    "lz4",
    "lzma",
    "m4a",
    "m4v",
    "mat",
    "mb",
    "mdb",
    "mdmp",
    "metallib",
    "mid",
    "midi",
    "mkv",
    "mlmodel",
    "mov",
    "mp3",
    "mp4",
    "mpeg",
    "mpg",
    "msgpack",
    "msi",
    "msix",
    "ncb",
    "nef",
    "node",
    "npy",
    "npz",
    "nupkg",
    "nvbin",
    "o",
    "obj",
    "ocx",
    "odp",
    "ods",
    "odt",
    "ogg",
    "ogv",
    "onnx",
    "opensdf",
    "optixir",
    "opus",
    "orc",
    "orf",
    "otf",
    "pak",
    "parquet",
    "pb",
    "pch",
    "pdb",
    "pdf",
    "pfa",
    "pfb",
    "pfm",
    "pickle",
    "pkg",
    "pkl",
    "png",
    "pptx",
    "psb",
    "psd",
    "pt",
    "pth",
    "pvr",
    "pyc",
    "pyo",
    "qcow2",
    "rar",
    "raw",
    "rdb",
    "res",
    "resource",
    "ress",
    "rlib",
    "rmeta",
    "rpm",
    "rw2",
    "safetensors",
    "sbr",
    "scr",
    "snupkg",
    "so",
    "spv",
    "sqlite",
    "sqlite3",
    "sst",
    "suo",
    "svgz",
    "swf",
    "sys",
    "tar",
    "tbz2",
    "tga",
    "tflite",
    "tgz",
    "tif",
    "tiff",
    "tlb",
    "ttc",
    "ttf",
    "txz",
    "uasset",
    "ubulk",
    "ucas",
    "umap",
    "unitypackage",
    "upk",
    "uptnl",
    "usdc",
    "usdz",
    "utoc",
    "vdb",
    "vhd",
    "vhdx",
    "vkcache",
    "vmdk",
    "vox",
    "vsix",
    "wasm",
    "wav",
    "webm",
    "webp",
    "whl",
    "wma",
    "wmv",
    "woff",
    "woff2",
    "xcf",
    "xlsx",
    "xpi",
    "xz",
    "z",
    "zip",
    "zst",
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
