//! Parallel `rayon` token boundary checks and >64 KiB buffer eviction/hydration tests.

use std::fs;

use cpdir::config::limits::{HARD_READ_BYTE_CAP, INLINE_CONTENT_CACHE_BYTES};
use cpdir::domain::FileDiscardReason;
use cpdir::output::render_strict_xml;
use cpdir::scanner::{
    ContentHydrationError, ScanConfig, hydrate_selected_contents,
    release_large_unselected_contents, scan_directory,
};
use cpdir::tokens::{TokenCounter, populate_token_counts};

use crate::common::scan_helpers::{find_node_id, scan_and_analyze};
use crate::common::workspace::WorkspaceBuilder;
use crate::common::xml_validator::parse_strict_xml;

#[test]
fn enforces_exact_threshold_boundary_and_parallel_rayon_batching() {
    let ws = WorkspaceBuilder::new();
    let counter = TokenCounter::o200k().expect("o200k");

    let mut expected_total = 0usize;
    let mut exact_limit_tokens = 0usize;
    for idx in 0..24 {
        let body = format!("// module {idx}\npub fn f_{idx}() -> usize {{ {idx} + 1 }}\n");
        let tok = counter.count(&body);
        if idx == 0 {
            exact_limit_tokens = tok;
        }
        expected_total += tok;
        ws.write_text(&format!("pkg_{}/mod_{idx}.rs", idx % 6), &body);
    }

    let over_body = "// module 0\npub fn f_0() -> usize { 0 + 1 } // extra\n";
    let over_tokens = counter.count(over_body);
    assert!(over_tokens > exact_limit_tokens);
    ws.write_text("over_boundary.rs", over_body);

    let mut scan = scan_directory(ws.root(), &ScanConfig::default()).expect("scan");
    populate_token_counts(&mut scan.tree, &scan.contents, exact_limit_tokens, counter);

    let over_id = find_node_id(&scan.tree, "over_boundary.rs");
    let metrics = scan.tree.compute_metrics();
    assert_eq!(metrics.counts.unread_files, 1);
    assert_eq!(metrics.counts.selected_tokens, expected_total);
    assert_eq!(metrics.rollups[over_id.index()].selected_tokens, 0);
    assert_eq!(metrics.rollups[over_id.index()].total_tokens, over_tokens);
}

#[test]
fn releases_large_unselected_buffers_and_hydrates_on_manual_selection() {
    let ws = WorkspaceBuilder::new();
    let row = "{\"id\":123,\"name\":\"alpha_beta\",\"active\":true},\n";
    let huge_payload = row.repeat(3_800);
    let large_ignored = "ignored line payload\n".repeat(4_000);

    ws.write_text(".gitignore", "logs/\n");
    ws.write_text("src/main.rs", "fn main() {}\n");
    ws.write_text("data/huge_52k.json", &huge_payload);
    ws.write_text("logs/large_ignored.log", &large_ignored);

    let mut scan = scan_and_analyze(&ws);
    let main_id = find_node_id(&scan.tree, "src/main.rs");
    let huge_id = find_node_id(&scan.tree, "data/huge_52k.json");
    let ignored_id = find_node_id(&scan.tree, "logs/large_ignored.log");

    let released = release_large_unselected_contents(
        &scan.tree,
        &mut scan.contents,
        INLINE_CONTENT_CACHE_BYTES,
    );
    assert_eq!(released, 2);
    assert!(scan.contents.contains_key(&main_id));
    assert!(!scan.contents.contains_key(&huge_id));
    assert!(!scan.contents.contains_key(&ignored_id));

    scan.tree.toggle_selection(huge_id);
    hydrate_selected_contents(
        ws.root(),
        &scan.tree,
        &mut scan.contents,
        HARD_READ_BYTE_CAP,
    )
    .expect("rehydrate selected >50k file");
    assert!(scan.contents.contains_key(&huge_id));
    assert!(!scan.contents.contains_key(&ignored_id));

    let xml = render_strict_xml(&scan.tree, &scan.contents, None).expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    assert!(
        parsed
            .files
            .iter()
            .any(|f| f.path == "data/huge_52k.json" && f.content == huge_payload)
    );

    fs::remove_file(ws.root().join("logs/large_ignored.log")).expect("remove log");
    scan.tree.toggle_selection(ignored_id);
    let err = hydrate_selected_contents(
        ws.root(),
        &scan.tree,
        &mut scan.contents,
        HARD_READ_BYTE_CAP,
    )
    .expect_err("deleted file must fail hydration cleanly");
    assert_eq!(
        err,
        ContentHydrationError::FileUnavailable {
            rel_path: "logs/large_ignored.log".to_owned(),
            reason: FileDiscardReason::Unreadable,
        }
    );
}
