//! End-to-end CLI binary, Explorer path normalization, and real clipboard round-trip tests.

mod common;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use common::workspace::{WorkspaceBuilder, mid_stream_corrupt_utf8_bytes, sample_png_bytes};
use common::xml_validator::{ParsedXmlFile, parse_strict_xml};
use cpdir::cli::{CliCommand, CliParseError, normalize_explorer_path, parse_cli_args};
use cpdir::config::defaults::{DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD, HARD_READ_BYTE_CAP};
use cpdir::domain::{DirectoryTree, NaturalSelection, TextFileData, TextFileState};
use cpdir::output::{copy_to_clipboard, render_strict_xml};
use cpdir::scanner::{ReadFileOutcome, ScanConfig, read_bounded_utf8, scan_directory};
use cpdir::tokens::{TokenCounter, populate_token_counts};

static CLIPBOARD_LOCK: Mutex<()> = Mutex::new(());

struct ClipboardRestoreGuard(Option<String>);

impl ClipboardRestoreGuard {
    fn capture() -> Self {
        let prev = Clipboard::new().ok().and_then(|mut c| c.get_text().ok());
        Self(prev)
    }
}

impl Drop for ClipboardRestoreGuard {
    fn drop(&mut self) {
        if let Some(prev) = self.0.take() {
            let _ = copy_to_clipboard(&prev);
        }
    }
}

fn read_clipboard_with_retry() -> String {
    for attempt in 0..5 {
        if let Ok(mut cb) = Clipboard::new()
            && let Ok(text) = cb.get_text()
        {
            return text;
        }
        if attempt < 4 {
            thread::sleep(Duration::from_millis(20));
        }
    }
    panic!("failed to read back text from system clipboard after retries");
}

#[test]
fn cli_normalizes_explorer_drive_root_quotes_unc_and_rejects_invalid_args() {
    assert_eq!(normalize_explorer_path(r#"C:""#), Ok(PathBuf::from(r"C:\")));
    assert_eq!(normalize_explorer_path(r#"d:""#), Ok(PathBuf::from(r"d:\")));
    assert_eq!(
        normalize_explorer_path(r#"C:\Users\Dev\My (Repo) [v1]""#),
        Ok(PathBuf::from(r"C:\Users\Dev\My (Repo) [v1]"))
    );
    assert_eq!(
        normalize_explorer_path(r#""C:\Quoted Folder\proj""#),
        Ok(PathBuf::from(r"C:\Quoted Folder\proj"))
    );
    assert_eq!(
        normalize_explorer_path(r#"\\nas\share\repo""#),
        Ok(PathBuf::from(r"\\nas\share\repo"))
    );
    assert_eq!(normalize_explorer_path("C:"), Ok(PathBuf::from("C:")));
    assert_eq!(normalize_explorer_path(""), Err(CliParseError::EmptyPath));

    assert_eq!(
        parse_cli_args(["cpdir"]),
        Ok(CliCommand::Run {
            target_dir: PathBuf::from(".")
        })
    );
    assert_eq!(
        parse_cli_args(["cpdir", "--", "-dash-folder"]),
        Ok(CliCommand::Run {
            target_dir: PathBuf::from("-dash-folder")
        })
    );
    assert_eq!(parse_cli_args(["cpdir", "--help"]), Ok(CliCommand::Help));
    assert_eq!(parse_cli_args(["cpdir", "-V"]), Ok(CliCommand::Version));
    assert_eq!(
        parse_cli_args(["cpdir", "--unknown"]),
        Err(CliParseError::UnknownFlag("--unknown".to_owned()))
    );
    assert_eq!(
        parse_cli_args(["cpdir", "one", "two"]),
        Err(CliParseError::UnexpectedArgument("two".to_owned()))
    );
}

#[test]
fn compiled_binary_reports_help_version_and_non_tty_errors_without_touching_stdout() {
    let bin = env!("CARGO_BIN_EXE_cpdir");

    let help_out = Command::new(bin).arg("--help").output().expect("run help");
    assert!(help_out.status.success());
    assert_eq!(String::from_utf8_lossy(&help_out.stdout), "");
    assert!(String::from_utf8_lossy(&help_out.stderr).contains("USAGE:"));

    let ver_out = Command::new(bin).arg("-V").output().expect("run version");
    assert!(ver_out.status.success());
    assert_eq!(String::from_utf8_lossy(&ver_out.stdout), "");
    assert!(String::from_utf8_lossy(&ver_out.stderr).contains("cpdir 0.1.0"));

    let bad_flag = Command::new(bin)
        .arg("--invalid-opt")
        .output()
        .expect("run bad flag");
    assert!(!bad_flag.status.success());
    assert_eq!(String::from_utf8_lossy(&bad_flag.stdout), "");

    let ws = WorkspaceBuilder::new();
    let plain_file = ws.write_text("not_a_dir.txt", "hello");
    let file_err = Command::new(bin)
        .arg(&plain_file)
        .output()
        .expect("run file target");
    assert!(!file_err.status.success());
    assert_eq!(String::from_utf8_lossy(&file_err.stdout), "");

    // Running interactive TUI over piped stdio fails cleanly without hanging.
    let non_tty = Command::new(bin)
        .arg(ws.root())
        .output()
        .expect("run non-tty dir target");
    assert!(!non_tty.status.success());
    assert_eq!(String::from_utf8_lossy(&non_tty.stdout), "");
    assert!(String::from_utf8_lossy(&non_tty.stderr).contains("interactive terminal"));
}

#[test]
fn full_scan_tokenize_xml_pipeline_writes_win32_clipboard_cleanly() {
    let _lock = CLIPBOARD_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let _restore = ClipboardRestoreGuard::capture();

    let ws = WorkspaceBuilder::new();
    let rust_body = "fn main() {\r\n    let ok = true;\r\n}\r\n";
    let hostile_body = "notes: 1 < 2 && 3 > 1 ]]> 🚀\n";

    ws.write_text("main.rs", rust_body);
    ws.write_text("notes & tags.txt", hostile_body);
    ws.write_bytes("empty.txt", &[]);
    ws.write_bytes("fake.ts", &sample_png_bytes());
    ws.write_bytes("broken.txt", &mid_stream_corrupt_utf8_bytes());
    ws.create_dir("sub_empty");
    ws.populate_files("node_modules/dep", 10, "mod");

    let mut scan = scan_directory(ws.root(), &ScanConfig::default()).expect("scan");
    let counter = TokenCounter::o200k().expect("o200k");
    populate_token_counts(
        &mut scan.tree,
        &scan.contents,
        DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD,
        counter,
    );

    let xml = render_strict_xml(&scan.tree, &scan.contents, None).expect("xml");
    copy_to_clipboard(&xml).expect("copy to clipboard");

    let clipboard_text = read_clipboard_with_retry();
    let parsed = parse_strict_xml(&clipboard_text).expect("clipboard xml must be valid");
    let dir_tree = parsed.directory_structure.expect("directory_structure");

    assert!(dir_tree.contains("sub_empty/"));
    assert!(dir_tree.contains("fake.ts"));
    assert!(dir_tree.contains("broken.txt"));
    assert!(!clipboard_text.contains("node_modules"));

    assert_eq!(
        parsed.files,
        vec![
            ParsedXmlFile {
                path: "empty.txt".to_owned(),
                content: String::new(),
            },
            ParsedXmlFile {
                path: "main.rs".to_owned(),
                content: rust_body.to_owned(),
            },
            ParsedXmlFile {
                path: "notes & tags.txt".to_owned(),
                content: hostile_body.to_owned(),
            },
        ]
    );
}

#[test]
fn clipboard_roundtrips_2_5_mib_unicode_xml_payload_without_truncation() {
    let _lock = CLIPBOARD_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let _restore = ClipboardRestoreGuard::capture();

    let ws = WorkspaceBuilder::new();
    let mut tree = DirectoryTree::new("large_repo");
    let mut contents = HashMap::new();
    let mut expected_files = Vec::new();
    let line = "pub fn compute_step(x: usize) -> bool { x < 100 && x > 0 } // 日本語 🚀\r\n";
    let chunk = line.repeat(6_500);

    for idx in 0..5 {
        let name = format!("shard_{idx}.rs");
        let path = ws.write_text(&name, &chunk);
        let ReadFileOutcome::Text { byte_len, content } =
            read_bounded_utf8(&path, HARD_READ_BYTE_CAP)
        else {
            panic!("expected readable large text file");
        };
        let fid = tree
            .add_text_file(
                DirectoryTree::ROOT,
                &name,
                TextFileData {
                    byte_len,
                    tokens: 120_000,
                    secret_hits: 0,
                    state: TextFileState::new(NaturalSelection::Included),
                },
            )
            .expect("add shard");
        contents.insert(fid, content.clone());
        expected_files.push(ParsedXmlFile {
            path: name,
            content,
        });
    }

    let xml_payload = render_strict_xml(&tree, &contents, None).expect("render large xml");
    assert!(xml_payload.len() > 2_000_000);

    copy_to_clipboard(&xml_payload).expect("copy 2.5 MiB payload to clipboard");
    let read_back = read_clipboard_with_retry();
    assert_eq!(read_back, xml_payload);

    let parsed = parse_strict_xml(&read_back).expect("large clipboard xml must parse cleanly");
    assert_eq!(parsed.files, expected_files);
}
