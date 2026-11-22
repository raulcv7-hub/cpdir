//! Deep directory stack synchronization and NTFS junction cycle resistance tests.

use std::fmt::Write as _;
use std::process::Command;

use cpdir::output::render_strict_xml;
use cpdir::scanner::{ScanConfig, ScanError, scan_directory};

use crate::common::workspace::WorkspaceBuilder;
use crate::common::xml_validator::parse_strict_xml;

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
