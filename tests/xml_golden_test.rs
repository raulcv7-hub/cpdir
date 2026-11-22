//! Golden real-disk ingestion and strict XML output verification tests.

mod common;

use std::collections::HashMap;

use common::workspace::{WorkspaceBuilder, sample_png_bytes};
use common::xml_validator::{ParsedXmlFile, parse_strict_xml};
use cpdir::config::defaults::HARD_READ_BYTE_CAP;
use cpdir::domain::{
    DirectoryTree, FileDiscardReason, NaturalExclusion, NaturalSelection, NodeId, PrunedDirReason,
    TextFileData, TextFileState,
};
use cpdir::output::{
    GitCommitEntry, GitContextSnapshot, GitDiffNumstatEntry, GitDiffStat, GitHotspotEntry,
    GitTagEntry, XmlWriteError, render_strict_xml,
};
use cpdir::scanner::{ReadFileOutcome, read_bounded_utf8};

fn ingest_text(path: &std::path::Path) -> (u64, String) {
    match read_bounded_utf8(path, HARD_READ_BYTE_CAP) {
        ReadFileOutcome::Text { byte_len, content } => (byte_len, content),
        ReadFileOutcome::Discarded(reason) => panic!("expected readable text, got {reason:?}"),
    }
}

#[test]
fn golden_workspace_preserves_collapsed_tree_empty_dirs_binaries_and_literal_xml() {
    let ws = WorkspaceBuilder::new();
    let main_body = "fn main() {\r\n    let x = \"a < b && c > d ]]> </file>\";\r\n}\r\n";
    let helper_body = "pub fn helper() -> &'static str { \"🚀 &amp; literal\" }\n";
    let odd_body = "  leading spaces and no trailing newline";

    let p_main = ws.write_text("src/main.rs", main_body);
    let p_helper = ws.write_text("src/collapsed/helper.rs", helper_body);
    let p_odd = ws.write_text("docs/a & b.txt", odd_body);
    let p_empty = ws.write_bytes("empty.txt", &[]);
    let p_env = ws.write_text("src/.env", "API_KEY=secret\n");
    let _p_png = ws.write_bytes("assets/logo.png", &sample_png_bytes());
    ws.create_dir("empty_dir");
    ws.populate_files("node_modules/pkg", 3, "dep");

    let mut tree = DirectoryTree::new("my_repo");
    tree.record_pruned_dir("node_modules", PrunedDirReason::DependencyOrVcs);

    let assets = tree
        .add_dir(DirectoryTree::ROOT, "assets", true)
        .expect("assets");
    tree.add_discarded_file(assets, "logo.png", FileDiscardReason::Binary)
        .expect("logo");

    let docs = tree
        .add_dir(DirectoryTree::ROOT, "docs", true)
        .expect("docs");
    let (odd_len, odd_text) = ingest_text(&p_odd);
    let id_odd = add_selected(&mut tree, docs, "a & b.txt", odd_len, 5);

    tree.add_dir(DirectoryTree::ROOT, "empty_dir", true)
        .expect("empty_dir");

    let src = tree.add_dir(DirectoryTree::ROOT, "src", true).expect("src");
    let collapsed = tree.add_dir(src, "collapsed", false).expect("collapsed");
    let (helper_len, helper_text) = ingest_text(&p_helper);
    let id_helper = add_selected(&mut tree, collapsed, "helper.rs", helper_len, 10);

    let (env_len, env_text) = ingest_text(&p_env);
    let id_env = tree
        .add_text_file(
            src,
            ".env",
            TextFileData {
                byte_len: env_len,
                tokens: 5,
                secret_hits: 1,
                state: TextFileState::new(NaturalSelection::Excluded(NaturalExclusion::GitIgnore)),
            },
        )
        .expect(".env");

    let (main_len, main_text) = ingest_text(&p_main);
    let id_main = add_selected(&mut tree, src, "main.rs", main_len, 20);

    let (empty_len, empty_text) = ingest_text(&p_empty);
    let id_empty = add_selected(&mut tree, DirectoryTree::ROOT, "empty.txt", empty_len, 0);

    let mut contents = HashMap::new();
    contents.insert(id_odd, odd_text);
    contents.insert(id_helper, helper_text);
    contents.insert(id_env, env_text);
    contents.insert(id_main, main_text);
    contents.insert(id_empty, empty_text);

    let xml = render_strict_xml(&tree, &contents, None).expect("xml render succeeds");
    let parsed = parse_strict_xml(&xml).expect("strict xml must parse cleanly");

    assert_eq!(
        parsed.directory_structure.as_deref(),
        Some(concat!(
            "\n    my_repo/\n",
            "      assets/\n",
            "        logo.png\n",
            "      docs/\n",
            "        a & b.txt\n",
            "      empty_dir/\n",
            "      src/\n",
            "        collapsed/\n",
            "          helper.rs\n",
            "        .env\n",
            "        main.rs\n",
            "      empty.txt\n",
            "  ",
        ))
    );
    assert!(!parsed.has_git_context);
    assert_eq!(
        parsed.files,
        vec![
            ParsedXmlFile {
                path: "docs/a & b.txt".to_owned(),
                content: odd_body.to_owned(),
            },
            ParsedXmlFile {
                path: "src/collapsed/helper.rs".to_owned(),
                content: helper_body.to_owned(),
            },
            ParsedXmlFile {
                path: "src/main.rs".to_owned(),
                content: main_body.to_owned(),
            },
            ParsedXmlFile {
                path: "empty.txt".to_owned(),
                content: String::new(),
            },
        ]
    );
    assert!(!xml.contains("node_modules"));
    assert!(!xml.contains("API_KEY=secret"));
}

#[test]
fn golden_git_context_escapes_hostile_xml_and_handles_empty_sections() {
    let tree = DirectoryTree::new("repo");
    let contents = HashMap::new();

    let git = GitContextSnapshot {
        branch: "feat/a&b<c>".to_owned(),
        working_tree_lines: vec!["M src/a&b.rs".to_owned()],
        working_diff_numstat: vec![
            GitDiffNumstatEntry {
                stat: GitDiffStat::Text {
                    added: 12,
                    deleted: 4,
                },
                rel_path: "src/a&b<1>.rs".to_owned(),
            },
            GitDiffNumstatEntry {
                stat: GitDiffStat::Binary,
                rel_path: "assets/logo&icon.png".to_owned(),
            },
        ],
        recent_commits: vec![GitCommitEntry {
            short_hash: "a1b2c3d".to_owned(),
            subject: "fix: escape <tag> & ]]>".to_owned(),
            body: Some("line1 <x>\nline2 & \"y\"".to_owned()),
        }],
        recent_tags: vec![GitTagEntry {
            name: "v0.4.0<beta>".to_owned(),
            summary: Some("release & notes".to_owned()),
        }],
        hotspots: vec![GitHotspotEntry {
            rel_path: "src/a&b.rs".to_owned(),
            commits: 7,
        }],
    };

    let xml = render_strict_xml(&tree, &contents, Some(&git)).expect("render with git");
    let parsed = parse_strict_xml(&xml).expect("git xml parses cleanly");
    assert!(parsed.has_git_context);
    assert_eq!(parsed.files, vec![]);
    assert!(xml.contains("<branch>feat/a&amp;b&lt;c&gt;</branch>"));
    assert!(xml.contains("<working_diff_numstat count=\"2\">"));
    assert!(xml.contains("      +12 -4 src/a&amp;b&lt;1&gt;.rs"));
    assert!(xml.contains("      binary assets/logo&amp;icon.png"));
    assert!(xml.contains("a1b2c3d | fix: escape &lt;tag&gt; &amp; ]]&gt;"));
    assert!(xml.contains("        line1 &lt;x&gt;"));
    assert!(xml.contains("<recent_tags count=\"1\">"));
    assert!(xml.contains("v0.4.0&lt;beta&gt; | release &amp; notes"));
    assert!(xml.contains("      src/a&amp;b.rs (7 commits)"));

    let clean_git = GitContextSnapshot {
        branch: "main".to_owned(),
        working_tree_lines: vec![],
        working_diff_numstat: vec![],
        recent_commits: vec![],
        recent_tags: vec![],
        hotspots: vec![],
    };
    let clean_xml = render_strict_xml(&tree, &contents, Some(&clean_git)).expect("clean git");
    assert!(clean_xml.contains("<working_tree/>"));
    assert!(!clean_xml.contains("<working_diff_numstat"));
    assert!(!clean_xml.contains("<recent_tags"));
    assert!(!clean_xml.contains("<hotspots>"));
    assert!(parse_strict_xml(&clean_xml).is_ok());
}

#[test]
fn sanitizes_illegal_xml10_controls_and_fails_fast_on_missing_content() {
    let ws = WorkspaceBuilder::new();
    let raw_log = "\x1B[32mok\x1B[0m\t\r\n\x01\x07\x0C\u{FFFE}\u{FFFF}";
    let log_path = ws.write_text("logs/ansi.log", raw_log);
    let (byte_len, text) = ingest_text(&log_path);

    let mut tree = DirectoryTree::new("logs_repo");
    let log_id = add_selected(&mut tree, DirectoryTree::ROOT, "ansi.log", byte_len, 10);

    let err =
        render_strict_xml(&tree, &HashMap::new(), None).expect_err("must reject missing body");
    assert_eq!(
        err,
        XmlWriteError::MissingFileContent {
            rel_path: "ansi.log".to_owned(),
        }
    );
    assert!(err.to_string().contains("ansi.log"));

    let mut contents = HashMap::new();
    contents.insert(log_id, text);
    let xml = render_strict_xml(&tree, &contents, None).expect("sanitized xml succeeds");
    let parsed = parse_strict_xml(&xml).expect("sanitized xml parses strictly");
    assert_eq!(
        parsed.files,
        vec![ParsedXmlFile {
            path: "ansi.log".to_owned(),
            content: "\u{FFFD}[32mok\u{FFFD}[0m\t\r\n\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}"
                .to_owned(),
        }]
    );
}

fn add_selected(
    tree: &mut DirectoryTree,
    parent: NodeId,
    name: &str,
    byte_len: u64,
    tokens: usize,
) -> NodeId {
    tree.add_text_file(
        parent,
        name,
        TextFileData {
            byte_len,
            tokens,
            secret_hits: 0,
            state: TextFileState::new(NaturalSelection::Included),
        },
    )
    .expect("valid text entry")
}
