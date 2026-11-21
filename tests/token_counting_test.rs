//! Real-disk token counting, parallel rayon batching, and >50k auto-deselect tests.

mod common;

use std::collections::BTreeMap;
use std::fs;

use common::workspace::{WorkspaceBuilder, sample_png_bytes};
use common::xml_validator::parse_strict_xml;
use cpdir::config::defaults::{DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD, HARD_READ_BYTE_CAP};
use cpdir::domain::metrics::InspectorReason;
use cpdir::domain::{
    DirectoryTree, FileDiscardReason, InspectorCategory, NodeId, NodeKind, UnselectedReason,
    WeightTier,
};
use cpdir::output::render_strict_xml;
use cpdir::scanner::{
    ContentHydrationError, INLINE_CONTENT_CACHE_BYTES, ScanConfig, hydrate_selected_contents,
    release_large_unselected_contents, scan_directory,
};
use cpdir::tokens::{TokenCounter, populate_token_counts};

fn find_node_id(tree: &DirectoryTree, rel_path: &str) -> NodeId {
    let idx = tree
        .nodes()
        .iter()
        .position(|n| n.rel_path == rel_path)
        .unwrap_or_else(|| panic!("missing node for {rel_path}"));

    if let Some(row) = tree
        .visible_rows()
        .into_iter()
        .find(|r| r.node_id.index() == idx)
    {
        return row.node_id;
    }

    for node in tree.nodes() {
        if let NodeKind::Directory(dir) = &node.kind {
            for &cid in &dir.children {
                if cid.index() == idx {
                    return cid;
                }
            }
        }
    }
    panic!("could not resolve NodeId for {rel_path}")
}

#[test]
fn corpus_token_rollups_handle_special_markers_and_ignored_folders() {
    let ws = WorkspaceBuilder::new();
    let gitignore_src = "ignored_dir/\n";
    let main_src = "fn main() {\n    println!(\"hola 🚀\");\n}\n";
    let parser_src = "pub fn parse(input: &str) -> usize { input.len() }\n";
    let special_txt = "prompt boundary <|endoftext|> and <|im_start|>system\n";
    let ignored_txt = "cached log line 1\ncached log line 2\n";

    ws.write_text(".gitignore", gitignore_src);
    ws.write_text("src/main.rs", main_src);
    ws.write_text("src/utils/parser.rs", parser_src);
    ws.write_text("notes/special.txt", special_txt);
    ws.write_text("ignored_dir/cache.log", ignored_txt);
    ws.write_bytes("assets/logo.png", &sample_png_bytes());

    let counter = TokenCounter::o200k().expect("o200k initializes cleanly");
    let mut scan = scan_directory(ws.root(), &ScanConfig::default()).expect("scan");
    populate_token_counts(
        &mut scan.tree,
        &scan.contents,
        DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD,
        counter,
    );

    let tok_gitignore = counter.count(gitignore_src);
    let tok_main = counter.count(main_src);
    let tok_parser = counter.count(parser_src);
    let tok_special = counter.count(special_txt);
    let tok_ignored = counter.count(ignored_txt);

    let gitignore_id = find_node_id(&scan.tree, ".gitignore");
    let src_id = find_node_id(&scan.tree, "src");
    let ignored_dir_id = find_node_id(&scan.tree, "ignored_dir");
    let special_id = find_node_id(&scan.tree, "notes/special.txt");

    let NodeKind::TextFile(gi_file) = &scan.tree.get(gitignore_id).expect("gi").kind else {
        panic!("expected text file for .gitignore");
    };
    assert_eq!(gi_file.tokens, tok_gitignore);

    let metrics = scan.tree.compute_metrics();
    assert_eq!(
        metrics.rollups[src_id.index()].selected_tokens,
        tok_main + tok_parser
    );
    assert_eq!(metrics.rollups[ignored_dir_id.index()].selected_tokens, 0);
    assert_eq!(
        metrics.rollups[ignored_dir_id.index()].total_tokens,
        tok_ignored
    );
    assert_eq!(
        metrics.counts.selected_tokens,
        tok_gitignore + tok_main + tok_parser + tok_special
    );
    assert!(metrics.rollups[special_id.index()].selected_tokens > 0);

    let xml = render_strict_xml(&scan.tree, &scan.contents, None).expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    assert!(
        parsed
            .files
            .iter()
            .any(|f| f.path == "notes/special.txt" && f.content == special_txt)
    );
}

#[test]
fn auto_deselects_real_file_over_50k_tokens_without_overwriting_gitignore() {
    let ws = WorkspaceBuilder::new();
    let row = "{\"id\":123,\"name\":\"alpha_beta\",\"active\":true},\n";
    let huge_payload = row.repeat(3_800);
    let small_payload = "pub const VERSION: &str = \"1.0\";\n";

    ws.write_text(".gitignore", "ignored_huge.json\n");
    ws.write_text("src/small.rs", small_payload);
    ws.write_text("data/huge_52k.json", &huge_payload);
    ws.write_text("ignored_huge.json", &huge_payload);

    let counter = TokenCounter::o200k().expect("o200k");
    let huge_tokens = counter.count(&huge_payload);
    assert!(
        huge_tokens > DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD,
        "fixture must exceed 50k tokens (got {huge_tokens})"
    );

    let mut scan = scan_directory(ws.root(), &ScanConfig::default()).expect("scan");
    populate_token_counts(
        &mut scan.tree,
        &scan.contents,
        DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD,
        counter,
    );

    let huge_id = find_node_id(&scan.tree, "data/huge_52k.json");
    let metrics = scan.tree.compute_metrics();
    assert_eq!(metrics.counts.read_files, 2);
    assert_eq!(metrics.counts.unread_files, 2);
    assert_eq!(
        metrics.rollups[huge_id.index()].display_weight_tier(false),
        WeightTier::Critical
    );

    let unread: BTreeMap<String, UnselectedReason> = scan
        .tree
        .inspector_items(InspectorCategory::Unread)
        .into_iter()
        .map(|item| match item.reason {
            InspectorReason::UnreadFile(r) => (item.rel_path, r),
            other => panic!("unexpected inspector reason {other:?}"),
        })
        .collect();
    assert_eq!(
        unread,
        BTreeMap::from([
            (
                "data/huge_52k.json".to_owned(),
                UnselectedReason::ExceedsTokenThreshold,
            ),
            ("ignored_huge.json".to_owned(), UnselectedReason::GitIgnore),
        ])
    );

    // User manually toggles >50k file ON: it enters XML literally.
    scan.tree.toggle_selection(huge_id);
    let after = scan.tree.compute_metrics();
    assert!(after.counts.selected_tokens > DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD);

    let xml = render_strict_xml(&scan.tree, &scan.contents, None).expect("xml");
    let parsed = parse_strict_xml(&xml).expect("valid xml");
    assert!(
        parsed
            .files
            .iter()
            .any(|f| f.path == "data/huge_52k.json" && f.content == huge_payload)
    );
}

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

    // Add one extra file whose token count is strictly exact_limit_tokens + 1.
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

    let counter = TokenCounter::o200k().expect("o200k");
    let mut scan = scan_directory(ws.root(), &ScanConfig::default()).expect("scan");
    populate_token_counts(
        &mut scan.tree,
        &scan.contents,
        DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD,
        counter,
    );

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

    // Selecting `data/huge_52k.json` rehydrates it transparently before strict XML emission.
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

    // Deleting a released unselected file on disk surfaces a clean error if later selected.
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
