//! Adversarial domain scenarios, 3000-level stack safety, and illegal tree construction tests.

use cpdir::domain::metrics::InspectorReason;
use cpdir::domain::{
    DirectoryTree, FileDiscardReason, FolderSelectionState, InspectorCategory, NaturalExclusion,
    NaturalSelection, NodeId, NodeKind, PrunedDirReason, TextFileData, TextFileState, TreeError,
    UnselectedReason, WeightTier,
};

struct MixedFixture {
    tree: DirectoryTree,
    src: NodeId,
    logs: NodeId,
    main_rs: NodeId,
    env_file: NodeId,
    dump_json: NodeId,
    debug_log: NodeId,
}

fn build_mixed_fixture() -> MixedFixture {
    let mut tree = DirectoryTree::new("repo");
    let src = tree.add_dir(DirectoryTree::ROOT, "src", true).expect("src");
    let logs = tree
        .add_dir(DirectoryTree::ROOT, "logs", true)
        .expect("logs");
    tree.add_dir(DirectoryTree::ROOT, "empty_dir", true)
        .expect("empty_dir");

    let main_rs = add_text(
        &mut tree,
        src,
        "main.rs",
        400,
        0,
        NaturalSelection::Included,
    );
    let env_file = add_text(
        &mut tree,
        src,
        ".env",
        50,
        1,
        NaturalSelection::Excluded(NaturalExclusion::GitIgnore),
    );
    let dump_json = add_text(
        &mut tree,
        src,
        "dump.json",
        65_000,
        0,
        NaturalSelection::Excluded(NaturalExclusion::ExceedsTokenThreshold),
    );
    tree.add_discarded_file(src, "logo.png", FileDiscardReason::Binary)
        .expect("logo.png");
    let debug_log = add_text(
        &mut tree,
        logs,
        "debug.log",
        100,
        0,
        NaturalSelection::Excluded(NaturalExclusion::DotIgnore),
    );
    tree.record_pruned_dir("node_modules", PrunedDirReason::DependencyOrVcs);

    MixedFixture {
        tree,
        src,
        logs,
        main_rs,
        env_file,
        dump_json,
        debug_log,
    }
}

fn add_text(
    tree: &mut DirectoryTree,
    parent: NodeId,
    name: &str,
    tokens: usize,
    secret_hits: usize,
    natural: NaturalSelection,
) -> NodeId {
    tree.add_text_file(
        parent,
        name,
        TextFileData {
            byte_len: u64::try_from(tokens).unwrap_or(0) * 4,
            tokens,
            secret_hits,
            state: TextFileState::new(natural),
        },
    )
    .expect("valid text fixture")
}

fn is_selected(tree: &DirectoryTree, id: NodeId) -> bool {
    matches!(
        tree.get(id).map(|n| &n.kind),
        Some(NodeKind::TextFile(f)) if f.state.is_selected()
    )
}

#[test]
fn mixed_folder_toggle_protects_secrets_and_oversized_files() {
    let mut f = build_mixed_fixture();

    let m0 = f.tree.compute_metrics();
    assert_eq!(m0.counts.read_files, 1);
    assert_eq!(m0.counts.unread_files, 3);
    assert_eq!(m0.counts.discarded_items, 2);
    assert_eq!(m0.counts.selected_tokens, 400);
    assert_eq!(m0.counts.active_secret_files, 0);
    assert_eq!(m0.counts.total_secret_files, 1);
    assert_eq!(
        m0.rollups[f.src.index()].folder_selection,
        FolderSelectionState::Partial
    );
    assert_eq!(
        m0.rollups[f.src.index()].display_weight_tier(true),
        WeightTier::Light
    );
    assert_eq!(
        m0.rollups[f.dump_json.index()].display_weight_tier(false),
        WeightTier::Critical
    );

    f.tree.toggle_selection(f.src);
    let m1 = f.tree.compute_metrics();
    assert_eq!(
        m1.rollups[f.src.index()].folder_selection,
        FolderSelectionState::NoneSelected
    );
    assert_eq!(m1.counts.selected_tokens, 0);
    assert_eq!(
        m1.rollups[f.src.index()].display_weight_tier(false),
        WeightTier::Critical
    );

    // Re-enabling mixed folder restores defaults without pulling `.env` or >50k dump.
    f.tree.toggle_selection(f.src);
    let m2 = f.tree.compute_metrics();
    assert_eq!(m2.counts.read_files, 1);
    assert_eq!(m2.counts.active_secret_files, 0);
    assert!(is_selected(&f.tree, f.main_rs));

    // Purely ignored folder turns on when toggled directly from empty.
    f.tree.toggle_selection(f.logs);
    assert_eq!(f.tree.compute_metrics().counts.read_files, 2);
    assert!(is_selected(&f.tree, f.debug_log));
    assert_eq!(
        f.tree
            .inspector_items(InspectorCategory::Unread)
            .into_iter()
            .map(|i| (i.rel_path, i.reason))
            .collect::<Vec<_>>(),
        vec![
            (
                "src/.env".to_owned(),
                InspectorReason::UnreadFile(UnselectedReason::GitIgnore),
            ),
            (
                "src/dump.json".to_owned(),
                InspectorReason::UnreadFile(UnselectedReason::ExceedsTokenThreshold),
            ),
        ]
    );

    // Secret alert counter tracks only selected secret-bearing files.
    f.tree.toggle_selection(f.env_file);
    assert_eq!(f.tree.compute_metrics().counts.active_secret_files, 1);
    f.tree.toggle_selection(f.env_file);
    assert_eq!(f.tree.compute_metrics().counts.active_secret_files, 0);
}

#[test]
fn deep_nesting_3000_levels_is_stack_safe_and_exact() {
    let mut tree = DirectoryTree::new("deep");
    let mut parent = DirectoryTree::ROOT;
    let mut dir_chain = Vec::with_capacity(3_000);

    for depth in 0..3_000 {
        parent = tree
            .add_dir(parent, format!("d{depth}"), true)
            .expect("valid deep dir");
        dir_chain.push(parent);
        if depth % 500 == 0 {
            add_text(
                &mut tree,
                parent,
                "leaf.rs",
                25,
                0,
                NaturalSelection::Included,
            );
        }
    }

    let metrics = tree.compute_metrics();
    assert_eq!(metrics.counts.read_files, 6);
    assert_eq!(metrics.counts.selected_tokens, 150);
    assert_eq!(tree.visible_rows().len(), 3_007);

    tree.toggle_selection(dir_chain[1_000]);
    let overrides = tree.collect_overrides();
    assert_eq!(overrides.get("d0/d1/d2"), None);
    assert_eq!(tree.compute_metrics().counts.read_files, 2);

    tree.toggle_selection(DirectoryTree::ROOT);
    assert_eq!(tree.compute_metrics().counts.read_files, 0);
}

#[test]
fn tree_builder_rejects_illegal_parent_and_empty_names() {
    let mut tree = DirectoryTree::new("root");
    let file_id = add_text(
        &mut tree,
        DirectoryTree::ROOT,
        "a.rs",
        3,
        0,
        NaturalSelection::Included,
    );
    let bin_id = tree
        .add_discarded_file(DirectoryTree::ROOT, "b.png", FileDiscardReason::Binary)
        .expect("bin");

    assert_eq!(
        tree.add_dir(DirectoryTree::ROOT, "", true),
        Err(TreeError::EmptyName)
    );
    assert_eq!(
        tree.add_dir(file_id, "nested", true),
        Err(TreeError::InvalidParent(file_id))
    );
    assert_eq!(
        tree.add_text_file(
            bin_id,
            "c.rs",
            TextFileData {
                byte_len: 1,
                tokens: 1,
                secret_hits: 0,
                state: TextFileState::new(NaturalSelection::Included),
            },
        ),
        Err(TreeError::InvalidParent(bin_id))
    );
}
