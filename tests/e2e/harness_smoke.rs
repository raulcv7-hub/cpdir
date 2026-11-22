//! Smoke verification tests ensuring the shared workspace, Git, and XML validator harness works.

use std::fs;

use crate::common::git_repo::GitRepoHarness;
use crate::common::workspace::{
    WorkspaceBuilder, mid_stream_corrupt_utf8_bytes, sample_png_bytes, utf16_le_with_bom_bytes,
};
use crate::common::xml_validator::{ParsedXmlFile, parse_strict_xml};

#[test]
fn workspace_harness_materializes_real_files_binaries_and_empty_dirs() {
    let ws = WorkspaceBuilder::new();
    let txt_path = ws.write_text("src/main.rs", "fn main() {}\n");
    let png_path = ws.write_bytes("assets/logo.png", &sample_png_bytes());
    let bad_utf8_path = ws.write_bytes("broken.txt", &mid_stream_corrupt_utf8_bytes());
    let utf16_path = ws.write_bytes("win16.txt", &utf16_le_with_bom_bytes("hola"));
    let empty_dir = ws.create_dir("empty_folder");
    ws.populate_files("node_modules/pkg", 5, "chunk");

    assert_eq!(
        fs::read_to_string(txt_path).expect("read text"),
        "fn main() {}\n"
    );
    assert_eq!(
        fs::read(png_path).expect("read png")[..4],
        [0x89, b'P', b'N', b'G']
    );
    assert!(std::str::from_utf8(&fs::read(bad_utf8_path).expect("read bad utf8")).is_err());
    assert_eq!(fs::read(utf16_path).expect("read utf16")[..2], [0xFF, 0xFE]);
    assert!(empty_dir.is_dir());
    assert!(ws.root().join("node_modules/pkg/chunk_4.js").is_file());
}

#[test]
fn git_harness_creates_real_history_and_isolates_local_config() {
    let ws = WorkspaceBuilder::new();
    ws.write_text("README.md", "# test\n");
    let git = GitRepoHarness::init(ws.root());
    let hash = git.commit_all("feat: initial", Some("body with <xml> & details"));
    git.append_raw_git_config("# harmless marker");

    assert_ne!(hash, "");
    let log = git.run_git(&["log", "-1", "--pretty=%s|%b"]);
    assert_eq!(log, "feat: initial|body with <xml> & details");
    let cfg = fs::read_to_string(ws.root().join(".git/config")).expect("read .git/config");
    assert!(cfg.contains("# harmless marker"));
}

#[test]
fn strict_xml_validator_preserves_hostile_entities_crlf_and_rejects_broken_xml() {
    let xml = concat!(
        "<cpdir_context>",
        "<directory_structure>src/\n  a&amp;b.rs</directory_structure>",
        "<git_context><branch>main</branch></git_context>",
        "<files>",
        "<file path=\"src/a&amp;b &quot;1&quot;.rs\">",
        "let s = &quot;a &lt; b &amp;&amp; c &gt; d&quot;;&#xD;\n",
        "</file>",
        "<file path=\"empty.txt\"/>",
        "</files>",
        "</cpdir_context>"
    );

    let parsed = parse_strict_xml(xml).expect("valid strict xml must parse");
    assert_eq!(
        parsed.directory_structure.as_deref(),
        Some("src/\n  a&b.rs")
    );
    assert!(parsed.has_git_context);
    assert_eq!(
        parsed.files,
        vec![
            ParsedXmlFile {
                path: "src/a&b \"1\".rs".to_owned(),
                content: "let s = \"a < b && c > d\";\r\n".to_owned(),
            },
            ParsedXmlFile {
                path: "empty.txt".to_owned(),
                content: String::new(),
            },
        ]
    );

    let broken = "<cpdir_context><files><file path=\"a.rs\">unclosed</cpdir_context>";
    assert!(parse_strict_xml(broken).is_err());

    let bad_entity =
        "<cpdir_context><files><file path=\"a.rs\">&unknown;</file></files></cpdir_context>";
    assert!(parse_strict_xml(bad_entity).is_err());
}
