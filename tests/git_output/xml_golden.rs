//! Golden real-disk ingestion and strict XML structure verification tests.

use std::collections::HashMap;
use std::path::Path;

use cpdir::config::limits::HARD_READ_BYTE_CAP;
use cpdir::domain::{
    DirectoryTree, FileDiscardReason, GitCommitEntry, GitContextSnapshot, GitDiffNumstatEntry,
    GitDiffStat, GitHotspotEntry, GitTagEntry, NaturalExclusion, NaturalSelection, NodeId,
    PrunedDirReason, TextFileData, TextFileState,
};
use cpdir::output::render_strict_xml;
use cpdir::scanner::{ReadFileOutcome, read_bounded_utf8};

use crate::common::workspace::{WorkspaceBuilder, sample_png_bytes};
use crate::common::xml_validator::{ParsedContextSummary, ParsedXmlFile, parse_strict_xml};

struct GoldenFixture {
    _ws: WorkspaceBuilder,
    tree: DirectoryTree,
    contents: HashMap<NodeId, String>,
    odd_body: String,
    helper_body: String,
    main_body: String,
}

fn ingest_text(path: &Path) -> (u64, String) {
    match read_bounded_utf8(path, HARD_READ_BYTE_CAP) {
        ReadFileOutcome::Text { byte_len, content } => (byte_len, content),
        ReadFileOutcome::Discarded(reason) => panic!("expected readable text, got {reason:?}"),
    }
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

fn build_golden_workspace_fixture() -> GoldenFixture {
    let ws = WorkspaceBuilder::new();
    let main_body =
        "fn main() {\r\n    let x = \"a < b && c > d ]]> </file>\";\r\n}\r\n".to_owned();
    let helper_body = "pub fn helper() -> &'static str { \"🚀 &amp; literal\" }\n".to_owned();
    let odd_body = "  leading spaces and no trailing newline".to_owned();

    let p_main = ws.write_text("src/main.rs", &main_body);
    let p_helper = ws.write_text("src/collapsed/helper.rs", &helper_body);
    let p_odd = ws.write_text("docs/a & b.txt", &odd_body);
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

    let contents = HashMap::from([
        (id_odd, odd_text),
        (id_helper, helper_text),
        (id_env, env_text),
        (id_main, main_text),
        (id_empty, empty_text),
    ]);

    GoldenFixture {
        _ws: ws,
        tree,
        contents,
        odd_body,
        helper_body,
        main_body,
    }
}

#[test]
fn golden_workspace_preserves_stacked_paths_empty_dirs_badges_and_cdata_xml() {
    let f = build_golden_workspace_fixture();
    let xml = render_strict_xml(&f.tree, &f.contents, None).expect("xml render succeeds");
    assert!(!xml.contains("&#xD;"));
    assert!(xml.contains("<![CDATA[pub fn helper()"));

    let parsed = parse_strict_xml(&xml).expect("strict xml must parse cleanly");
    assert_eq!(
        parsed.context_summary,
        Some(ParsedContextSummary {
            root: "my_repo".to_owned(),
            root_path: None,
            included_files: 4,
            unselected_files: 1,
            discarded_files: 1,
            pruned_dirs: Some("node_modules".to_owned()),
        })
    );
    let dir_tree = parsed.directory_structure.as_deref().expect("dir tree");
    assert_eq!(
        dir_tree,
        concat!(
            "\n    assets/logo.png [binary]\n",
            "    docs/a & b.txt\n",
            "    empty_dir/\n",
            "    src/collapsed/helper.rs\n",
            "    src/.env [unselected: .gitignore] (5) [!]\n",
            "    src/main.rs\n",
            "    empty.txt\n",
            "  ",
        )
    );
    assert!(!dir_tree.contains("node_modules"));
    assert!(!parsed.has_git_context);
    assert_eq!(
        parsed.files,
        vec![
            ParsedXmlFile {
                path: "docs/a & b.txt".to_owned(),
                content: f.odd_body,
            },
            ParsedXmlFile {
                path: "src/collapsed/helper.rs".to_owned(),
                content: f.helper_body,
            },
            ParsedXmlFile {
                path: "src/main.rs".to_owned(),
                content: f.main_body.replace("\r\n", "\n"),
            },
            ParsedXmlFile {
                path: "empty.txt".to_owned(),
                content: String::new(),
            },
        ]
    );
    assert!(!xml.contains("API_KEY=secret"));
}

#[test]
fn golden_selected_secret_file_still_emits_warning_marker_in_directory_structure() {
    let mut tree = DirectoryTree::new("sec_tree");
    let id = tree
        .add_text_file(
            DirectoryTree::ROOT,
            "keys.ts",
            TextFileData {
                byte_len: 20,
                tokens: 6,
                secret_hits: 1,
                state: TextFileState::new(NaturalSelection::Included),
            },
        )
        .expect("add keys.ts");
    let contents = HashMap::from([(id, "export const k = 1;\n".to_owned())]);

    let xml = render_strict_xml(&tree, &contents, None).expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    let dir_tree = parsed.directory_structure.expect("directory_structure");
    assert!(dir_tree.contains("    keys.ts [!]\n"));
    assert_eq!(parsed.files.len(), 1);
}

#[test]
fn golden_cdata_avoids_double_escaping_in_svgs_scripts_and_crlf_docs() {
    let svg_body = "<svg viewBox=\"0 0 10 10\"><path d=\"M0 0\"/>&gt;</svg>\r\n";
    let sh_body = "#!/usr/bin/env bash\r\nsvgo \"$1\" >/dev/null 2>&1\r\n";
    let bracket_body = "nested ]]> and ]]]]> inside file\n";

    let mut tree = DirectoryTree::new("svg_repo");
    let svg_id = add_selected(&mut tree, DirectoryTree::ROOT, "icon.svg", 64, 15);
    let sh_id = add_selected(&mut tree, DirectoryTree::ROOT, "opt.sh", 64, 12);
    let br_id = add_selected(&mut tree, DirectoryTree::ROOT, "edge.txt", 36, 8);

    let contents = HashMap::from([
        (svg_id, svg_body.to_owned()),
        (sh_id, sh_body.to_owned()),
        (br_id, bracket_body.to_owned()),
    ]);

    let xml = render_strict_xml(&tree, &contents, None).expect("xml");
    assert!(xml.contains(r#"<![CDATA[<svg viewBox="0 0 10 10"><path d="M0 0"/>&gt;</svg>"#));
    assert!(xml.contains(r#"svgo "$1" >/dev/null 2>&1"#));
    assert!(!xml.contains("&lt;svg"));
    assert!(!xml.contains("&amp;gt;"));
    assert!(!xml.contains("&gt;/dev/null"));
    assert!(!xml.contains("&#xD;"));

    let parsed = parse_strict_xml(&xml).expect("valid cdata xml");
    assert_eq!(
        parsed.files,
        vec![
            ParsedXmlFile {
                path: "icon.svg".to_owned(),
                content: svg_body.replace("\r\n", "\n"),
            },
            ParsedXmlFile {
                path: "opt.sh".to_owned(),
                content: sh_body.replace("\r\n", "\n"),
            },
            ParsedXmlFile {
                path: "edge.txt".to_owned(),
                content: bracket_body.to_owned(),
            },
        ]
    );
}

#[test]
fn golden_summary_caps_pruned_dirs_and_renders_all_discard_and_unselected_badges() {
    let mut tree = DirectoryTree::new("badges_repo");
    for idx in 0..14 {
        tree.record_pruned_dir(
            format!("pkg_{idx}/node_modules"),
            PrunedDirReason::DependencyOrVcs,
        );
    }

    tree.add_discarded_file(DirectoryTree::ROOT, "blob.bin", FileDiscardReason::Binary)
        .expect("bin");
    tree.add_discarded_file(
        DirectoryTree::ROOT,
        "broken.txt",
        FileDiscardReason::InvalidUtf8,
    )
    .expect("utf8");
    tree.add_discarded_file(
        DirectoryTree::ROOT,
        "dump.csv",
        FileDiscardReason::ExceedsHardCap {
            byte_len: 6_000_000,
        },
    )
    .expect("cap");
    tree.add_discarded_file(
        DirectoryTree::ROOT,
        "locked.dat",
        FileDiscardReason::Unreadable,
    )
    .expect("unreadable");

    let mut off_state = TextFileState::new(NaturalSelection::Included);
    off_state.set_selected(false);
    let mut contents = HashMap::new();
    let entries = [
        ("user_off.rs", off_state, 42),
        (
            "dot_ign.log",
            TextFileState::new(NaturalSelection::Excluded(NaturalExclusion::DotIgnore)),
            120,
        ),
        (
            "big.json",
            TextFileState::new(NaturalSelection::Excluded(
                NaturalExclusion::ExceedsTokenThreshold,
            )),
            55_200,
        ),
        (
            "pruned_dep.js",
            TextFileState::new(NaturalSelection::Excluded(NaturalExclusion::Pruned)),
            0,
        ),
    ];
    for (name, state, tokens) in entries {
        let id = tree
            .add_text_file(
                DirectoryTree::ROOT,
                name,
                TextFileData {
                    byte_len: 10,
                    tokens,
                    secret_hits: 0,
                    state,
                },
            )
            .expect("add text");
        contents.insert(id, String::new());
    }

    let xml = render_strict_xml(&tree, &contents, None).expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    let summary = parsed.context_summary.expect("summary");
    assert_eq!(summary.included_files, 0);
    assert_eq!(summary.unselected_files, 4);
    assert_eq!(summary.discarded_files, 4);
    let pruned = summary.pruned_dirs.expect("pruned_dirs");
    assert!(pruned.ends_with(", ... (+2 more)"));
    assert!(pruned.contains("pkg_11/node_modules"));
    assert!(!pruned.contains("pkg_12/node_modules"));

    let dir_tree = parsed.directory_structure.expect("dir_tree");
    assert!(dir_tree.contains("blob.bin [binary]"));
    assert!(dir_tree.contains("broken.txt [UTF8]"));
    assert!(dir_tree.contains("dump.csv [>5MB]"));
    assert!(dir_tree.contains("locked.dat [unreadable]"));
    assert!(dir_tree.contains("user_off.rs [unselected: off] (42)"));
    assert!(dir_tree.contains("dot_ign.log [unselected: .ignore] (120)"));
    assert!(dir_tree.contains("big.json [unselected: >50k] (55.2k)"));
    assert!(dir_tree.contains("pruned_dep.js [unselected: pruned] (0)"));
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
            body: Some("line1 <x>\r\nline2 & \"y\"".to_owned()),
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
    assert!(!xml.contains("&#xD;"));
    let parsed = parse_strict_xml(&xml).expect("git xml parses cleanly");
    assert_eq!(
        parsed.context_summary,
        Some(ParsedContextSummary {
            root: "repo".to_owned(),
            root_path: None,
            included_files: 0,
            unselected_files: 0,
            discarded_files: 0,
            pruned_dirs: None,
        })
    );
    assert_eq!(parsed.directory_structure.as_deref(), Some(""));
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
