//! Multi-level `.ignore` > `.gitignore` precedence, pruning, and NTFS cycle golden tests.

mod common;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use common::workspace::{WorkspaceBuilder, mid_stream_corrupt_utf8_bytes, sample_png_bytes};
use common::xml_validator::{ParsedXmlFile, parse_strict_xml};
use cpdir::domain::metrics::InspectorReason;
use cpdir::domain::{InspectorCategory, NodeKind, PrunedDirReason, UnselectedReason};
use cpdir::output::render_strict_xml;
use cpdir::scanner::walker::resolve_root_name;
use cpdir::scanner::{ScanConfig, ScanError, scan_directory};

fn build_precedence_workspace() -> WorkspaceBuilder {
    let ws = WorkspaceBuilder::new();
    ws.write_text(".gitignore", "*.log\nlogs/\n");
    ws.write_text(".ignore", "!logs/important.log\ndocs/drafts/\nprivate/\n");

    ws.write_text("docs/guide.md", "# Guide\n");
    ws.write_text("docs/drafts/wip.md", "# Draft hidden by root .ignore\n");
    ws.create_dir("empty_top");

    ws.write_text("logs/drop.log", "ignored by root .gitignore\n");
    ws.write_text("logs/important.log", "rescued by root .ignore!\n");
    ws.write_text("logs/.ignore", "!sub/rescue.log\n");
    ws.write_text("logs/sub/rescue.log", "rescued by nested logs/.ignore!\n");

    ws.write_text("private/.gitignore", "!try_leak.txt\n");
    ws.write_text("private/try_leak.txt", "must stay hidden by root .ignore\n");

    ws.write_text("services/.gitignore", "*.tmp\n!keep.tmp\n");
    ws.write_text("services/.ignore", "keep.tmp\n!override.log\n");
    ws.write_text("services/keep.tmp", "re-excluded by services/.ignore\n");
    ws.write_text("services/override.log", "rescued by services/.ignore\n");

    ws.write_text("src/main.rs", "fn main() {}\n");
    ws.write_text("src/bin/cli.rs", "fn cli() {}\n");
    ws.create_dir("src/empty_nested");
    ws.write_bytes("assets/logo.png", &sample_png_bytes());
    ws.write_bytes("src/bad.txt", &mid_stream_corrupt_utf8_bytes());

    ws.populate_files("node_modules/pkg", 250, "dep");
    ws.populate_files("services/rust_svc/target/debug", 80, "art");
    ws.populate_files(".git/objects/pack", 10, "obj");
    ws
}

#[test]
fn golden_multilevel_ignore_precedence_pruning_and_xml_emission() {
    let ws = build_precedence_workspace();
    let scan = scan_directory(ws.root(), &ScanConfig::default()).expect("scan succeeds");

    let pruned: Vec<(&str, PrunedDirReason)> = scan
        .tree
        .pruned_dirs()
        .iter()
        .map(|p| (p.rel_path.as_str(), p.reason))
        .collect();
    assert_eq!(
        pruned,
        vec![
            (".git", PrunedDirReason::DependencyOrVcs),
            ("node_modules", PrunedDirReason::DependencyOrVcs),
            ("services/rust_svc/target", PrunedDirReason::DependencyOrVcs),
        ]
    );

    let unread_map: BTreeMap<String, UnselectedReason> = scan
        .tree
        .inspector_items(InspectorCategory::Unread)
        .into_iter()
        .map(|item| match item.reason {
            InspectorReason::UnreadFile(r) => (item.rel_path, r),
            other => panic!("expected unread file reason, got {other:?}"),
        })
        .collect();
    assert_eq!(
        unread_map,
        BTreeMap::from([
            ("docs/drafts/wip.md".to_owned(), UnselectedReason::DotIgnore),
            ("logs/.ignore".to_owned(), UnselectedReason::GitIgnore),
            ("logs/drop.log".to_owned(), UnselectedReason::GitIgnore),
            ("private/.gitignore".to_owned(), UnselectedReason::DotIgnore),
            (
                "private/try_leak.txt".to_owned(),
                UnselectedReason::DotIgnore
            ),
            ("services/keep.tmp".to_owned(), UnselectedReason::DotIgnore),
        ])
    );

    let logs_node = scan
        .tree
        .nodes()
        .iter()
        .find(|n| n.rel_path == "logs")
        .expect("logs dir present");
    assert!(matches!(
        &logs_node.kind,
        NodeKind::Directory(d) if !d.is_expanded
    ));

    let metrics = scan.tree.compute_metrics();
    assert_eq!(metrics.counts.read_files, 10);
    assert_eq!(metrics.counts.unread_files, 6);
    assert_eq!(metrics.counts.discarded_items, 5);

    let xml = render_strict_xml(&scan.tree, &scan.contents, None).expect("xml write");
    let parsed = parse_strict_xml(&xml).expect("strict xml must parse");
    let dir_tree = parsed.directory_structure.expect("directory_structure");

    for expected_entry in [
        "empty_top/",
        "empty_nested/",
        "logo.png",
        "bad.txt",
        "drop.log",
    ] {
        assert!(
            dir_tree.contains(expected_entry),
            "missing {expected_entry} in directory_structure"
        );
    }
    assert!(!xml.contains("node_modules"));
    assert!(!xml.contains("services/rust_svc/target"));

    let emitted_paths: Vec<String> = parsed.files.iter().map(|f| f.path.clone()).collect();
    assert_eq!(
        emitted_paths,
        vec![
            "docs/guide.md",
            "logs/sub/rescue.log",
            "logs/important.log",
            "services/.gitignore",
            "services/.ignore",
            "services/override.log",
            "src/bin/cli.rs",
            "src/main.rs",
            ".gitignore",
            ".ignore",
        ]
    );
    assert!(parsed.files.contains(&ParsedXmlFile {
        path: "logs/sub/rescue.log".to_owned(),
        content: "rescued by nested logs/.ignore!\n".to_owned(),
    }));
}

#[test]
fn preserves_source_nested_build_and_vendor_while_pruning_package_outputs() {
    let ws = WorkspaceBuilder::new();
    ws.write_text("src/build/gen.rs", "pub fn gen() {}\n");
    ws.write_text("internal/vendor/helper.go", "package vendor\n");
    ws.populate_files("apps/web/dist", 15, "bundle");
    ws.populate_files("infra/.terraform/providers", 10, "tf");

    let scan = scan_directory(ws.root(), &ScanConfig::default()).expect("scan");
    let xml = render_strict_xml(&scan.tree, &scan.contents, None).expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");

    let emitted: Vec<String> = parsed.files.into_iter().map(|f| f.path).collect();
    assert_eq!(
        emitted,
        vec!["internal/vendor/helper.go", "src/build/gen.rs"]
    );
    assert!(!xml.contains("apps/web/dist"));
    assert!(!xml.contains("infra/.terraform"));

    if cfg!(windows) {
        assert_eq!(resolve_root_name(Path::new(r"C:\")), "C:");
    }
}

#[test]
fn case_sensitivity_config_controls_ignore_matching() {
    let ws = WorkspaceBuilder::new();
    ws.write_text(".gitignore", "Secret.TXT\n");
    ws.write_text(".ignore", "Drafts/\n");
    ws.write_text("secret.txt", "hidden on windows\n");
    ws.write_text("DRAFTS/note.md", "hidden on windows\n");

    let insens = scan_directory(
        ws.root(),
        &ScanConfig {
            case_insensitive_ignores: true,
            ..ScanConfig::default()
        },
    )
    .expect("case-insensitive scan");
    assert_eq!(insens.tree.compute_metrics().counts.read_files, 2);
    assert_eq!(insens.tree.compute_metrics().counts.unread_files, 2);

    let sens = scan_directory(
        ws.root(),
        &ScanConfig {
            case_insensitive_ignores: false,
            ..ScanConfig::default()
        },
    )
    .expect("case-sensitive scan");
    assert_eq!(sens.tree.compute_metrics().counts.read_files, 4);
    assert_eq!(sens.tree.compute_metrics().counts.unread_files, 0);
}

#[test]
fn deep_45_level_backtracking_keeps_ignore_stack_synchronized() {
    let ws = WorkspaceBuilder::new();
    ws.write_text(".gitignore", "*.tmp\n");

    let mut path_prefix = String::new();
    for depth in 0..45 {
        if !path_prefix.is_empty() {
            path_prefix.push('/');
        }
        let _ = write!(path_prefix, "d{depth}");
        if depth == 20 {
            ws.write_text(&format!("{path_prefix}/.ignore"), "!keep.tmp\n");
        } else if depth == 35 {
            ws.write_text(&format!("{path_prefix}/.ignore"), "keep.tmp\n");
        }
    }
    ws.write_text("d0/d1/drop.tmp", "drop at depth 2\n");
    ws.write_text(
        &format!(
            "{}/keep.tmp",
            (0..=25)
                .map(|i| format!("d{i}"))
                .collect::<Vec<_>>()
                .join("/")
        ),
        "rescued at depth 26\n",
    );
    ws.write_text(
        &format!("{path_prefix}/keep.tmp"),
        "re-ignored at depth 45\n",
    );
    ws.write_text(
        "sibling/after.tmp",
        "root .gitignore still active after deep pop\n",
    );

    let scan = scan_directory(ws.root(), &ScanConfig::default()).expect("deep scan");
    let xml = render_strict_xml(&scan.tree, &scan.contents, None).expect("xml write");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    let paths: Vec<String> = parsed.files.into_iter().map(|f| f.path).collect();

    assert!(paths.iter().any(|p| p.ends_with("d25/keep.tmp")));
    assert!(!paths.iter().any(|p| p.ends_with("d44/keep.tmp")));
    assert!(!paths.iter().any(|p| p == "d0/d1/drop.tmp"));
    assert!(!paths.iter().any(|p| p == "sibling/after.tmp"));
}

#[test]
fn rejects_non_directory_roots_and_avoids_ntfs_junction_cycles() {
    let ws = WorkspaceBuilder::new();
    let file_path = ws.write_text("single.txt", "content");
    let missing_path = ws.root().join("missing_dir");

    assert!(matches!(
        scan_directory(&file_path, &ScanConfig::default()),
        Err(ScanError::NotADirectory(_))
    ));
    assert!(matches!(
        scan_directory(&missing_path, &ScanConfig::default()),
        Err(ScanError::NotADirectory(_))
    ));

    let cycle_ws = WorkspaceBuilder::new();
    cycle_ws.write_text("keep.txt", "safe");
    let junction_path = cycle_ws.root().join("loop_junction");
    let junction_out = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction_path)
        .arg(cycle_ws.root())
        .output();
    if let Ok(out) = junction_out
        && out.status.success()
    {
        assert!(junction_path.exists());
    }

    let scan =
        scan_directory(cycle_ws.root(), &ScanConfig::default()).expect("must not loop on junction");
    let xml = render_strict_xml(&scan.tree, &scan.contents, None).expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    assert_eq!(parsed.files.len(), 1);
    assert_eq!(parsed.files[0].path, "keep.txt");
}
