//! Property-based round-trip and adversarial path/content tests for strict XML emission.

mod common;

use std::collections::HashMap;

use common::workspace::WorkspaceBuilder;
use common::xml_validator::{ParsedXmlFile, parse_strict_xml};
use cpdir::config::defaults::HARD_READ_BYTE_CAP;
use cpdir::domain::{
    DirectoryTree, FileDiscardReason, NaturalExclusion, NaturalSelection, NodeKind, TextFileData,
    TextFileState,
};
use cpdir::output::render_strict_xml;
use cpdir::scanner::{ReadFileOutcome, read_bounded_utf8};
use proptest::prelude::*;

fn expected_xml10_sanitized(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if matches!(
                ch,
                '\t' | '\n'
                    | '\r'
                    | '\u{0020}'..='\u{D7FF}'
                    | '\u{E000}'..='\u{FFFD}'
                    | '\u{10000}'..='\u{10FFFF}'
            ) {
                ch
            } else {
                '\u{FFFD}'
            }
        })
        .collect()
}

fn ntfs_safe_fragment_strategy() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-zA-Z0-9 _&'();=+@#%!~.-]{1,12}").expect("valid regex")
}

fn hostile_attr_name_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            Just('a'),
            Just('&'),
            Just('<'),
            Just('>'),
            Just('"'),
            Just('\''),
            Just('\t'),
            Just('\n'),
            Just('\r'),
            Just('🚀'),
        ],
        1..18,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

proptest! {
    #[test]
    fn arbitrary_disk_files_roundtrip_bit_for_bit_through_xml_validator(
        entries in prop::collection::vec(
            (ntfs_safe_fragment_strategy(), any::<String>()),
            1..10,
        ),
    ) {
        let ws = WorkspaceBuilder::new();
        let mut tree = DirectoryTree::new("fuzz_ws");
        let mut contents = HashMap::new();
        let mut expected_files = Vec::new();

        for (idx, (frag, raw_body)) in entries.into_iter().enumerate() {
            let safe_name = format!("f_{idx}_{}.txt", frag.trim_end_matches(['.', ' ']));
            let body = raw_body.replace('\0', "");
            let disk_body = body.strip_prefix('\u{FEFF}').unwrap_or(&body).to_owned();
            let path = ws.write_text(&safe_name, &disk_body);

            let (byte_len, ingested) = match read_bounded_utf8(&path, HARD_READ_BYTE_CAP) {
                ReadFileOutcome::Text { byte_len, content } => (byte_len, content),
                ReadFileOutcome::Discarded(r) => {
                    return Err(TestCaseError::fail(format!("unexpected discard: {r:?}")));
                }
            };
            prop_assert_eq!(&ingested, &disk_body);

            let node_id = tree
                .add_text_file(
                    DirectoryTree::ROOT,
                    &safe_name,
                    TextFileData {
                        byte_len,
                        tokens: 10,
                        secret_hits: 0,
                        state: TextFileState::new(NaturalSelection::Included),
                    },
                )
                .expect("add file");
            contents.insert(node_id, ingested);
            expected_files.push(ParsedXmlFile {
                path: safe_name,
                content: expected_xml10_sanitized(&disk_body),
            });
        }

        let xml = render_strict_xml(&tree, &contents, None).expect("xml write");
        let parsed = parse_strict_xml(&xml).map_err(TestCaseError::fail)?;
        prop_assert_eq!(parsed.files, expected_files);
    }

    #[test]
    fn hostile_in_memory_paths_with_angle_brackets_and_quotes_roundtrip_strictly(
        dir_name in hostile_attr_name_strategy(),
        file_name in hostile_attr_name_strategy(),
        body in any::<String>(),
    ) {
        let mut tree = DirectoryTree::new("hostile&<root>");
        let dir = tree
            .add_dir(DirectoryTree::ROOT, &dir_name, false)
            .expect("dir");
        let file_id = tree
            .add_text_file(
                dir,
                &file_name,
                TextFileData {
                    byte_len: u64::try_from(body.len()).unwrap_or(0),
                    tokens: 1,
                    secret_hits: 0,
                    state: TextFileState::new(NaturalSelection::Included),
                },
            )
            .expect("file");

        let mut contents = HashMap::new();
        contents.insert(file_id, body.clone());
        let expected_path = expected_xml10_sanitized(&format!("{dir_name}/{file_name}"));
        let expected_body = expected_xml10_sanitized(&body);

        let xml = render_strict_xml(&tree, &contents, None).expect("xml write");
        let parsed = parse_strict_xml(&xml).map_err(TestCaseError::fail)?;
        prop_assert_eq!(
            parsed.files,
            vec![ParsedXmlFile {
                path: expected_path,
                content: expected_body,
            }]
        );
    }

    #[test]
    fn collapse_and_selection_states_emit_exact_selected_files_and_full_tree(
        flags in prop::collection::vec((any::<bool>(), any::<bool>(), any::<bool>()), 1..25),
    ) {
        let mut tree = DirectoryTree::new("repo");
        let mut contents = HashMap::new();
        let mut expected_selected_paths = Vec::new();
        let mut all_names = Vec::new();

        for (idx, (dir_expanded, is_selected, is_discarded)) in flags.into_iter().enumerate() {
            let dir_name = format!("dir_{idx}");
            let dir_id = tree
                .add_dir(DirectoryTree::ROOT, &dir_name, dir_expanded)
                .expect("dir");
            all_names.push(format!("{dir_name}/"));

            let file_name = format!("item_{idx}.rs");
            all_names.push(file_name.clone());

            if is_discarded {
                tree.add_discarded_file(dir_id, &file_name, FileDiscardReason::Binary)
                    .expect("bin");
            } else {
                let nat = if is_selected {
                    NaturalSelection::Included
                } else {
                    NaturalSelection::Excluded(NaturalExclusion::GitIgnore)
                };
                let fid = tree
                    .add_text_file(
                        dir_id,
                        &file_name,
                        TextFileData {
                            byte_len: 5,
                            tokens: 2,
                            secret_hits: 0,
                            state: TextFileState::new(nat),
                        },
                    )
                    .expect("text");
                contents.insert(fid, format!("// {idx}\n"));
                if is_selected {
                    expected_selected_paths.push(format!("{dir_name}/{file_name}"));
                }
            }
        }

        let xml = render_strict_xml(&tree, &contents, None).expect("xml write");
        let parsed = parse_strict_xml(&xml).map_err(TestCaseError::fail)?;
        let dir_tree = parsed.directory_structure.expect("directory_structure present");

        for name in all_names {
            prop_assert!(dir_tree.contains(&name), "missing {name} in directory_structure");
        }
        let emitted_paths: Vec<String> = parsed.files.into_iter().map(|f| f.path).collect();
        prop_assert_eq!(emitted_paths, expected_selected_paths);
        let _ = tree.nodes().iter().filter(|n| matches!(n.kind, NodeKind::TextFile(_))).count();
    }
}
