//! Property-based tests verifying rollup invariants and persistence evolution in `cpdir::domain`.

use std::collections::{BTreeMap, HashMap, HashSet};

use cpdir::domain::{
    DirectoryTree, FileDiscardReason, FolderSelectionState, InspectorCategory, NaturalExclusion,
    NaturalSelection, NodeId, NodeKind, NodeRollup, PrunedDirReason, TextFileData, TextFileState,
};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum GenKind {
    Dir(bool),
    Text(NaturalSelection, usize, usize),
    Discarded(FileDiscardReason),
}

#[derive(Debug, Clone)]
struct GenNode {
    parent_slot: usize,
    kind: GenKind,
}

#[derive(Debug, Clone, Copy)]
enum TreeOp {
    ToggleExpand(usize),
    ToggleSelection(usize),
    SelectAllDefault,
    UnselectAll,
}

fn gen_node_strategy() -> impl Strategy<Value = GenNode> {
    let nat = prop_oneof![
        4 => Just(NaturalSelection::Included),
        1 => Just(NaturalSelection::Excluded(NaturalExclusion::DotIgnore)),
        1 => Just(NaturalSelection::Excluded(NaturalExclusion::GitIgnore)),
        1 => Just(NaturalSelection::Excluded(NaturalExclusion::ExceedsTokenThreshold)),
    ];
    let disc = prop_oneof![
        Just(FileDiscardReason::Binary),
        Just(FileDiscardReason::InvalidUtf8),
        Just(FileDiscardReason::ExceedsHardCap {
            byte_len: 6_000_000
        }),
        Just(FileDiscardReason::Unreadable),
    ];
    let kind = prop_oneof![
        2 => any::<bool>().prop_map(GenKind::Dir),
        5 => (nat, 0usize..=80_000usize, 0usize..=3usize)
            .prop_map(|(n, t, s)| GenKind::Text(n, t, s)),
        1 => disc.prop_map(GenKind::Discarded),
    ];
    (any::<usize>(), kind).prop_map(|(parent_slot, kind)| GenNode { parent_slot, kind })
}

fn tree_op_strategy() -> impl Strategy<Value = TreeOp> {
    prop_oneof![
        3 => any::<usize>().prop_map(TreeOp::ToggleExpand),
        5 => any::<usize>().prop_map(TreeOp::ToggleSelection),
        1 => Just(TreeOp::SelectAllDefault),
        1 => Just(TreeOp::UnselectAll),
    ]
}

fn build_fixture(
    specs: &[GenNode],
    pruned_count: usize,
) -> (DirectoryTree, Vec<NodeId>, Vec<NodeId>) {
    let mut tree = DirectoryTree::new("workspace");
    let mut all_ids = vec![DirectoryTree::ROOT];
    let mut dir_ids = vec![DirectoryTree::ROOT];

    for (idx, spec) in specs.iter().enumerate() {
        let parent = dir_ids[spec.parent_slot % dir_ids.len()];
        let name = format!("entry_{idx}");
        let id = match spec.kind {
            GenKind::Dir(expanded) => {
                let d = tree.add_dir(parent, name, expanded).expect("valid dir");
                dir_ids.push(d);
                d
            }
            GenKind::Text(natural, tokens, secret_hits) => tree
                .add_text_file(
                    parent,
                    format!("{name}.rs"),
                    TextFileData {
                        byte_len: u64::try_from(tokens).unwrap_or(0) * 4,
                        tokens,
                        secret_hits,
                        state: TextFileState::new(natural),
                    },
                )
                .expect("valid text"),
            GenKind::Discarded(reason) => tree
                .add_discarded_file(parent, format!("{name}.bin"), reason)
                .expect("valid discarded"),
        };
        all_ids.push(id);
    }

    for idx in 0..pruned_count {
        tree.record_pruned_dir(
            format!("dep_{idx}/node_modules"),
            PrunedDirReason::DependencyOrVcs,
        );
    }
    (tree, all_ids, dir_ids)
}

fn apply_op(tree: &mut DirectoryTree, all_ids: &[NodeId], op: TreeOp) {
    match op {
        TreeOp::ToggleExpand(i) => {
            tree.toggle_expand(all_ids[i % all_ids.len()]);
        }
        TreeOp::ToggleSelection(i) => tree.toggle_selection(all_ids[i % all_ids.len()]),
        TreeOp::SelectAllDefault => tree.select_all_default(),
        TreeOp::UnselectAll => tree.unselect_all(),
    }
}

fn assert_all_invariants(tree: &DirectoryTree) -> Result<(), TestCaseError> {
    let metrics = tree.compute_metrics();
    let root = metrics.rollups[DirectoryTree::ROOT.index()];
    prop_assert_eq!(metrics.counts.read_files, root.selected_files);
    prop_assert_eq!(metrics.counts.unread_files, root.unread_files);
    prop_assert_eq!(metrics.counts.selected_tokens, root.selected_tokens);
    prop_assert_eq!(
        metrics.counts.discarded_items,
        root.discarded_files + tree.pruned_dirs().len()
    );
    prop_assert!(metrics.counts.active_secret_files <= metrics.counts.total_secret_files);
    prop_assert_eq!(
        tree.inspector_items(InspectorCategory::Discarded).len(),
        metrics.counts.discarded_items
    );
    prop_assert_eq!(
        tree.inspector_items(InspectorCategory::Unread).len(),
        metrics.counts.unread_files
    );

    for (idx, node) in tree.nodes().iter().enumerate() {
        if let NodeKind::Directory(dir) = &node.kind {
            assert_dir_rollup(&dir.children, &metrics.rollups, metrics.rollups[idx])?;
        }
    }
    assert_visible_rows(tree)
}

fn assert_dir_rollup(
    children: &[NodeId],
    rollups: &[NodeRollup],
    dir_r: NodeRollup,
) -> Result<(), TestCaseError> {
    let (mut sel_tok, mut tot_tok, mut sel_f, mut unr_f, mut disc_f) = (0, 0, 0, 0, 0);
    for &cid in children {
        let cr = rollups[cid.index()];
        sel_tok += cr.selected_tokens;
        tot_tok += cr.total_tokens;
        sel_f += cr.selected_files;
        unr_f += cr.unread_files;
        disc_f += cr.discarded_files;
    }
    prop_assert_eq!(dir_r.selected_tokens, sel_tok);
    prop_assert_eq!(dir_r.total_tokens, tot_tok);
    prop_assert_eq!(dir_r.selected_files, sel_f);
    prop_assert_eq!(dir_r.unread_files, unr_f);
    prop_assert_eq!(dir_r.discarded_files, disc_f);
    let expected = match (sel_f, unr_f) {
        (0, 0) => FolderSelectionState::NoTextFiles,
        (_, 0) => FolderSelectionState::AllSelected,
        (0, _) => FolderSelectionState::NoneSelected,
        (_, _) => FolderSelectionState::Partial,
    };
    prop_assert_eq!(dir_r.folder_selection, expected);
    Ok(())
}

fn assert_visible_rows(tree: &DirectoryTree) -> Result<(), TestCaseError> {
    let rows = tree.visible_rows();
    prop_assert_eq!(rows.first().map(|r| r.node_id), Some(DirectoryTree::ROOT));
    let mut seen = HashSet::new();
    let mut depths = HashMap::new();

    for row in &rows {
        prop_assert!(seen.insert(row.node_id));
        depths.insert(row.node_id, row.depth);
        let node = tree.get(row.node_id).expect("node exists");
        if let Some(pid) = node.parent {
            let parent = tree.get(pid).expect("parent exists");
            let NodeKind::Directory(parent_dir) = &parent.kind else {
                return Err(TestCaseError::fail("non-directory parent in visible_rows"));
            };
            prop_assert!(parent_dir.is_expanded);
            let p_depth = depths
                .get(&pid)
                .copied()
                .ok_or_else(|| TestCaseError::fail("parent missing before child"))?;
            prop_assert_eq!(row.depth, p_depth + 1);
        } else {
            prop_assert_eq!(row.depth, 0);
        }
    }
    Ok(())
}

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
        let ok_id = run2.tree_add_text_fixture(
            dir_id,
            format!("new_ok_{i}.rs"),
            NaturalSelection::Included,
        );
        new_ok.push((dir_id, ok_id));
        let ign_id = run2.tree_add_text_fixture(
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

trait FixtureExt {
    fn tree_add_text_fixture(
        &mut self,
        parent: NodeId,
        name: String,
        nat: NaturalSelection,
    ) -> NodeId;
}

impl FixtureExt for DirectoryTree {
    fn tree_add_text_fixture(
        &mut self,
        parent: NodeId,
        name: String,
        nat: NaturalSelection,
    ) -> NodeId {
        self.add_text_file(
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
}

proptest! {
    #[test]
    fn arbitrary_trees_and_operations_preserve_rollup_and_visibility_invariants(
        specs in prop::collection::vec(gen_node_strategy(), 0..60),
        pruned_count in 0usize..5,
        ops in prop::collection::vec(tree_op_strategy(), 1..40),
    ) {
        let (mut tree, all_ids, _) = build_fixture(&specs, pruned_count);
        assert_all_invariants(&tree)?;
        for op in ops {
            apply_op(&mut tree, &all_ids, op);
            assert_all_invariants(&tree)?;
        }
    }

    #[test]
    fn override_persistence_round_trip_and_new_file_evolution(
        specs in prop::collection::vec(gen_node_strategy(), 1..45),
        ops in prop::collection::vec(tree_op_strategy(), 1..30),
    ) {
        check_persistence_evolution(&specs, &ops)?;
    }
}
