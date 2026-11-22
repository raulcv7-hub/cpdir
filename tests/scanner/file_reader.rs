//! Real-disk ingestion tests for UTF-8 BOMs, binaries, legacy encodings, and size caps.

use std::path::Path;

use cpdir::config::limits::HARD_READ_BYTE_CAP;
use cpdir::config::rules::{is_known_binary_extension, is_pruned_dir_name, should_prune_dir};
use cpdir::domain::FileDiscardReason;
use cpdir::scanner::{ReadFileOutcome, read_bounded_utf8};

use crate::common::workspace::{
    WorkspaceBuilder, mid_stream_corrupt_utf8_bytes, sample_png_bytes, utf16_le_with_bom_bytes,
};

#[test]
fn reads_utf8_boms_crlf_cjk_ansi_logs_and_empty_files() {
    let ws = WorkspaceBuilder::new();
    let mut bom_payload = vec![0xEF, 0xBB, 0xBF];
    bom_payload.extend_from_slice("fn café() { /* 日本語 🚀 */ }\r\n".as_bytes());
    let bom_file = ws.write_bytes("src/unicode.rs", &bom_payload);
    let bom_only = ws.write_bytes("bom_only.txt", &[0xEF, 0xBB, 0xBF]);
    let double_bom = ws.write_bytes("double_bom.txt", b"\xEF\xBB\xBF\xEF\xBB\xBFkeep");
    let ansi_log = ws.write_bytes("app.log", b"\x1B[31mERROR\x1B[0m\tline1\r\n");
    let empty_file = ws.write_bytes("empty.txt", &[]);

    assert_eq!(
        read_bounded_utf8(&bom_file, HARD_READ_BYTE_CAP),
        ReadFileOutcome::Text {
            byte_len: u64::try_from(bom_payload.len()).expect("len fits u64"),
            content: "fn café() { /* 日本語 🚀 */ }\r\n".to_owned(),
        }
    );
    assert_eq!(
        read_bounded_utf8(&bom_only, HARD_READ_BYTE_CAP),
        ReadFileOutcome::Text {
            byte_len: 3,
            content: String::new(),
        }
    );
    assert_eq!(
        read_bounded_utf8(&double_bom, HARD_READ_BYTE_CAP),
        ReadFileOutcome::Text {
            byte_len: 10,
            content: "\u{FEFF}keep".to_owned(),
        }
    );
    assert_eq!(
        read_bounded_utf8(&ansi_log, HARD_READ_BYTE_CAP),
        ReadFileOutcome::Text {
            byte_len: 22,
            content: "\x1B[31mERROR\x1B[0m\tline1\r\n".to_owned(),
        }
    );
    assert_eq!(
        read_bounded_utf8(&empty_file, HARD_READ_BYTE_CAP),
        ReadFileOutcome::Text {
            byte_len: 0,
            content: String::new(),
        }
    );
}

#[test]
fn discards_binaries_across_extensions_case_magic_headers_and_null_positions() {
    let ws = WorkspaceBuilder::new();
    let png = sample_png_bytes();
    let binary_cases: &[(&str, &[u8])] = &[
        ("src/fake_module.ts", &png),
        ("data/fake.json", b"PK\x03\x04\x14\x00\x00\x00"),
        ("docs/fake.txt", b"SQLite format 3\x00"),
        ("elf_no_ext", b"\x7FELF\x02\x01\x01\x00"),
        ("pe_no_ext", b"MZ\x90\x00\x03\x00\x00\x00"),
        ("null_start.rs", b"\x00fn main() {}"),
        ("null_middle.rs", b"fn main() {\x00}"),
        ("null_end.rs", b"fn main() {}\x00"),
        ("assets/PHOTO.PNG", b"plain ascii text"),
        ("dist/App.EXE", b"plain ascii text"),
        ("pkg/Archive.ZiP", b"plain ascii text"),
        ("bin/Module.WASM", b"plain ascii text"),
    ];
    for &(rel, raw) in binary_cases {
        let path = ws.write_bytes(rel, raw);
        assert_eq!(
            read_bounded_utf8(&path, HARD_READ_BYTE_CAP),
            ReadFileOutcome::Discarded(FileDiscardReason::Binary),
            "expected binary discard for {rel}"
        );
    }

    let text_cases: &[(&str, &str)] = &[
        ("assets/icon.svg", "<svg viewBox=\"0 0 10 10\"/>"),
        ("Dockerfile", "FROM scratch\n"),
        ("Makefile", "all:\n\techo ok\n"),
        (".gitignore", "/target/\n"),
        ("odd.tar.rs", "pub fn tar_helper() {}\n"),
    ];
    for &(rel, text) in text_cases {
        let path = ws.write_text(rel, text);
        assert_eq!(
            read_bounded_utf8(&path, HARD_READ_BYTE_CAP),
            ReadFileOutcome::Text {
                byte_len: u64::try_from(text.len()).expect("len fits u64"),
                content: text.to_owned(),
            },
            "expected readable text for {rel}"
        );
    }
}

#[test]
fn discards_legacy_cp1252_corrupt_utf8_surrogates_and_non_utf8_boms() {
    let ws = WorkspaceBuilder::new();
    let mid_corrupt = mid_stream_corrupt_utf8_bytes();
    let le_utf16 = utf16_le_with_bom_bytes("secret=1");

    let invalid_cases: &[(&str, &[u8])] = &[
        ("broken.rs", &mid_corrupt),
        ("latin1_cafe.txt", b"caf\xE9"),
        ("cp1252_enie.txt", b"Espa\xF1a"),
        ("trunc_2byte.txt", &[b'x', 0xC3]),
        ("trunc_3byte.txt", &[b'x', 0xE2, 0x82]),
        ("trunc_4byte.txt", &[b'x', 0xF0, 0x9F, 0x9A]),
        ("wtf8_surrogate.txt", &[0xED, 0xA0, 0x80]),
        ("bom_utf16_le.txt", &le_utf16),
        ("bom_utf16_be.txt", &[0xFE, 0xFF, 0x00, b'A']),
        (
            "bom_utf32_le.txt",
            &[0xFF, 0xFE, 0x00, 0x00, b'A', 0x00, 0x00, 0x00],
        ),
        (
            "bom_utf32_be.txt",
            &[0x00, 0x00, 0xFE, 0xFF, 0x00, 0x00, 0x00, b'A'],
        ),
    ];

    for &(rel, raw) in invalid_cases {
        let path = ws.write_bytes(rel, raw);
        assert_eq!(
            read_bounded_utf8(&path, HARD_READ_BYTE_CAP),
            ReadFileOutcome::Discarded(FileDiscardReason::InvalidUtf8),
            "expected InvalidUtf8 discard for {rel}"
        );
    }
}

#[test]
fn enforces_hard_byte_cap_boundaries_directories_and_missing_files() {
    let ws = WorkspaceBuilder::new();
    let cap_usize = usize::try_from(HARD_READ_BYTE_CAP).expect("5 MiB fits usize");

    let below_cap = ws.write_bytes("below.txt", &vec![b'a'; cap_usize - 1]);
    let at_cap = ws.write_bytes("at_cap.txt", &vec![b'a'; cap_usize]);
    let above_cap = ws.write_bytes("above.sql", &vec![b'a'; cap_usize + 1]);
    let empty_png = ws.write_bytes("zero.png", &[]);
    let folder_path = ws.create_dir("real_dir");
    let missing_path = ws.root().join("does_not_exist.rs");

    assert!(matches!(
        read_bounded_utf8(&below_cap, HARD_READ_BYTE_CAP),
        ReadFileOutcome::Text { byte_len, .. } if byte_len == HARD_READ_BYTE_CAP - 1
    ));
    assert!(matches!(
        read_bounded_utf8(&at_cap, HARD_READ_BYTE_CAP),
        ReadFileOutcome::Text { byte_len, .. } if byte_len == HARD_READ_BYTE_CAP
    ));
    assert_eq!(
        read_bounded_utf8(&above_cap, HARD_READ_BYTE_CAP),
        ReadFileOutcome::Discarded(FileDiscardReason::ExceedsHardCap {
            byte_len: HARD_READ_BYTE_CAP + 1,
        })
    );
    assert_eq!(
        read_bounded_utf8(&empty_png, HARD_READ_BYTE_CAP),
        ReadFileOutcome::Discarded(FileDiscardReason::Binary)
    );
    assert_eq!(
        read_bounded_utf8(&folder_path, HARD_READ_BYTE_CAP),
        ReadFileOutcome::Discarded(FileDiscardReason::Unreadable)
    );
    assert_eq!(
        read_bounded_utf8(&missing_path, HARD_READ_BYTE_CAP),
        ReadFileOutcome::Discarded(FileDiscardReason::Unreadable)
    );
}

#[test]
fn defaults_pruning_and_binary_extension_tables_preserve_safe_folders() {
    for dir_name in [
        ".GIT",
        "Node_Modules",
        "TARGET",
        "__PYCACHE__",
        ".Venv",
        ".IDEA",
    ] {
        assert!(is_pruned_dir_name(dir_name), "should prune {dir_name}");
    }
    for dir_name in ["bin", "src", "tests", ".vscode", ".github", "assets"] {
        assert!(!is_pruned_dir_name(dir_name), "must keep {dir_name}");
    }
    assert!(should_prune_dir("apps/web/dist", "dist"));
    assert!(should_prune_dir("services/api/build", "build"));
    assert!(should_prune_dir("infra/.terraform", ".terraform"));
    assert!(should_prune_dir("src/.git", ".git"));
    assert!(!should_prune_dir("src/build", "build"));
    assert!(!should_prune_dir("internal/vendor", "vendor"));
    assert!(!should_prune_dir("lib/dist", "dist"));

    assert!(is_known_binary_extension(Path::new("nested/FILE.DLL")));
    assert!(!is_known_binary_extension(Path::new("nested/icon.SVG")));
    assert!(!is_known_binary_extension(Path::new("Cargo.lock")));
}
