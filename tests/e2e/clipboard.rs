//! Real OS clipboard write/read round-trip tests including 2.5 MiB Unicode XML payloads.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use cpdir::config::limits::HARD_READ_BYTE_CAP;
use cpdir::domain::{DirectoryTree, NaturalSelection, TextFileData, TextFileState};
use cpdir::output::{copy_to_clipboard, render_strict_xml};
use cpdir::scanner::{ReadFileOutcome, read_bounded_utf8};

use crate::common::scan_helpers::scan_and_analyze;
use crate::common::workspace::{WorkspaceBuilder, mid_stream_corrupt_utf8_bytes, sample_png_bytes};
use crate::common::xml_validator::{ParsedXmlFile, parse_strict_xml};

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

    let scan = scan_and_analyze(&ws);
    let xml = render_strict_xml(&scan.tree, &scan.contents, None).expect("xml");
    copy_to_clipboard(&xml).expect("copy to clipboard");

    let clipboard_text = read_clipboard_with_retry();
    let parsed = parse_strict_xml(&clipboard_text).expect("clipboard xml must be valid");
    let summary = parsed.context_summary.expect("context_summary");
    assert_eq!(summary.included_files, 3);
    assert_eq!(summary.unselected_files, 0);
    assert_eq!(summary.discarded_files, 2);
    assert_eq!(summary.pruned_dirs.as_deref(), Some("node_modules"));

    let dir_tree = parsed.directory_structure.expect("directory_structure");
    assert!(dir_tree.contains("sub_empty/"));
    assert!(dir_tree.contains("fake.ts [binary]"));
    assert!(dir_tree.contains("broken.txt [UTF8]"));
    assert!(!dir_tree.contains("node_modules"));

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
