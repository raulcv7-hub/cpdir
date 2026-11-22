//! Property-based tests verifying delta override round-trip and unseen file evolution.

use std::collections::BTreeMap;

use cpdir::domain::{
    DirectoryTree, NaturalExclusion, NaturalSelection, NodeId, NodeKind, TextFileData,
    TextFileState,
};
use proptest::prelude::*;

use crate::properties::{
    GenNode, TreeOp, apply_op, build_fixture, gen_node_strategy, tree_op_strategy,
};

fn selected_by_path(tree: &DirectoryTree) -> BTreeMap<String, bool> {
    tree.nodes()
        .iter()
        .filter_map(|node| match &node.kind {
            NodeKind::TextFile(f) => Some((node.rel_path.clone(), f.state.is_selected())),
            _ => None,
        })
        .collect()
}

fn is_dir_excluded(tree: &DirectoryTree, mut id: NodeId) -> bool {
    loop {
        let node = tree.get(id).expect("dir exists");
        if let NodeKind::Directory(dir) = &node.kind {
            match dir.folder_override {
                Some(false) => return true,
                Some(true) => return false,
                None => {}
            }
        }
        match node.parent {
            Some(pid) => id = pid,
            None => return false,
        }
    }
}

fn add_new_text(
    tree: &mut DirectoryTree,
    parent: NodeId,
    name: String,
    nat: NaturalSelection,
) -> NodeId {
    tree.add_text_file(
        parent,
        name,
        TextFileData {
            byte_len: 80,
            tokens: 20,
            secret_hits: 0,
            state: TextFileState::new(nat),
        },
    )
    .expect("valid added file")
}

fn check_persistence_evolution(specs: &[GenNode], ops: &[TreeOp]) -> Result<(), TestCaseError> {
    let (mut run1, all_ids1, _) = build_fixture(specs, 1);
    for &op in ops {
        apply_op(&mut run1, &all_ids1, op);
    }
    let before = selected_by_path(&run1);
    let saved = run1.collect_overrides();

    let (mut run2, _, dir_ids2) = build_fixture(specs, 1);
    let mut new_ok = Vec::new();
    let mut new_ign = Vec::new();
    for (i, &dir_id) in dir_ids2.iter().enumerate() {
        let ok_id = add_new_text(
            &mut run2,
            dir_id,
            format!("new_ok_{i}.rs"),
            NaturalSelection::Included,
        );
        new_ok.push((dir_id, ok_id));
        let ign_id = add_new_text(
            &mut run2,
            dir_id,
            format!("new_ign_{i}.log"),
            NaturalSelection::Excluded(NaturalExclusion::GitIgnore),
        );
        new_ign.push(ign_id);
    }

    run2.apply_overrides(&saved);
    let after = selected_by_path(&run2);
    for (path, was_sel) in &before {
        prop_assert_eq!(after.get(path), Some(was_sel));
    }
    for (dir_id, file_id) in new_ok {
        let sel = matches!(
            run2.get(file_id).map(|n| &n.kind),
            Some(NodeKind::TextFile(f)) if f.state.is_selected()
        );
        prop_assert_eq!(sel, !is_dir_excluded(&run2, dir_id));
    }
    for file_id in new_ign {
        let sel = matches!(
            run2.get(file_id).map(|n| &n.kind),
            Some(NodeKind::TextFile(f)) if f.state.is_selected()
        );
        prop_assert!(!sel);
    }
    Ok(())
}

proptest! {
    #[test]
    fn override_persistence_round_trip_and_new_file_evolution(
        specs in prop::collection::vec(gen_node_strategy(), 1..45),
        ops in prop::collection::vec(tree_op_strategy(), 1..30),
    ) {
        check_persistence_evolution(&specs, &ops)?;
    }
}
