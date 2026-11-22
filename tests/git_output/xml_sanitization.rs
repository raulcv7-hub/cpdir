//! XML 1.0 illegal control-character sanitization and missing-content fail-fast tests.

use std::collections::HashMap;

use cpdir::config::limits::HARD_READ_BYTE_CAP;
use cpdir::domain::{DirectoryTree, NaturalSelection, TextFileData, TextFileState};
use cpdir::output::{XmlWriteError, render_strict_xml};
use cpdir::scanner::{ReadFileOutcome, read_bounded_utf8};

use crate::common::workspace::WorkspaceBuilder;
use crate::common::xml_validator::{ParsedXmlFile, parse_strict_xml};

#[test]
fn sanitizes_illegal_xml10_controls_and_fails_fast_on_missing_content() {
    let ws = WorkspaceBuilder::new();
    let raw_log = "\x1B[32mok\x1B[0m\t\r\n\x01\x07\x0C\u{FFFE}\u{FFFF}";
    let log_path = ws.write_text("logs/ansi.log", raw_log);
    let ReadFileOutcome::Text { byte_len, content } =
        read_bounded_utf8(&log_path, HARD_READ_BYTE_CAP)
    else {
        panic!("expected readable log file");
    };

    let mut tree = DirectoryTree::new("logs_repo");
    let log_id = tree
        .add_text_file(
            DirectoryTree::ROOT,
            "ansi.log",
            TextFileData {
                byte_len,
                tokens: 10,
                secret_hits: 0,
                state: TextFileState::new(NaturalSelection::Included),
            },
        )
        .expect("add ansi.log");

    let err =
        render_strict_xml(&tree, &HashMap::new(), None).expect_err("must reject missing body");
    assert_eq!(
        err,
        XmlWriteError::MissingFileContent {
            rel_path: "ansi.log".to_owned(),
        }
    );
    assert!(err.to_string().contains("ansi.log"));

    let contents = HashMap::from([(log_id, content)]);
    let xml = render_strict_xml(&tree, &contents, None).expect("sanitized xml succeeds");
    assert!(!xml.contains("&#xD;"));
    let parsed = parse_strict_xml(&xml).expect("sanitized xml parses strictly");
    assert_eq!(
        parsed.files,
        vec![ParsedXmlFile {
            path: "ansi.log".to_owned(),
            content: "\u{FFFD}[32mok\u{FFFD}[0m\t\n\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}"
                .to_owned(),
        }]
    );
}
