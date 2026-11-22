//! Real-disk `o200k_base` token rollups, special markers, and >50k threshold tests.

use std::collections::BTreeMap;

use cpdir::config::limits::DEFAULT_TOKEN_AUTO_DESELECT_THRESHOLD;
use cpdir::domain::{InspectorEntryKind, InspectorTab, NodeKind, UnselectedReason, WeightTier};
use cpdir::output::render_strict_xml;
use cpdir::tokens::TokenCounter;

use crate::common::scan_helpers::{find_node_id, scan_and_analyze};
use crate::common::workspace::{WorkspaceBuilder, sample_png_bytes};
use crate::common::xml_validator::parse_strict_xml;

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
    let scan = scan_and_analyze(&ws);

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

    let mut scan = scan_and_analyze(&ws);
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
        .inspector_entries(&metrics, InspectorTab::Unread)
        .into_iter()
        .filter_map(|item| match item.kind {
            InspectorEntryKind::UnreadFile { reason, .. } => Some((item.rel_path, reason)),
            _ => None,
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
