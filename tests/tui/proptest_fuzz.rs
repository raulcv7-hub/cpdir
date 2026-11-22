//! Property-based fuzzing of `TuiAppState`, mouse/key dispatch, and `TestBackend` rendering.

use std::collections::HashMap;

use cpdir::domain::{
    DirectoryTree, FileDiscardReason, NaturalExclusion, NaturalSelection, NodeId, PrunedDirReason,
    SecretFinding, SecretRuleKind, TextFileData, TextFileState,
};
use cpdir::output::render_strict_xml;
use cpdir::tui::{
    AppExitAction, TuiAppState, TuiViewportAreas, handle_key_event, handle_mouse_event,
};
use proptest::prelude::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::common::tui_harness::draw_screen_with_layout;
use crate::common::xml_validator::parse_strict_xml;

#[derive(Debug, Clone)]
enum FuzzNode {
    Dir(bool),
    Text(NaturalSelection, usize, bool),
    Discarded,
}

#[derive(Debug, Clone, Copy)]
enum FuzzEvent {
    Key(KeyCode),
    Wheel(bool),
    Click(u16, u16),
}

fn node_strategy() -> impl Strategy<Value = (usize, FuzzNode)> {
    let nat = prop_oneof![
        3 => Just(NaturalSelection::Included),
        1 => Just(NaturalSelection::Excluded(NaturalExclusion::DotIgnore)),
        1 => Just(NaturalSelection::Excluded(NaturalExclusion::GitIgnore)),
        1 => Just(NaturalSelection::Excluded(NaturalExclusion::ExceedsTokenThreshold)),
        1 => Just(NaturalSelection::Excluded(NaturalExclusion::Pruned)),
    ];
    let node = prop_oneof![
        2 => any::<bool>().prop_map(FuzzNode::Dir),
        5 => (nat, 10usize..=60_000usize, any::<bool>())
            .prop_map(|(n, t, sec)| FuzzNode::Text(n, t, sec)),
        1 => Just(FuzzNode::Discarded),
    ];
    (any::<usize>(), node)
}

fn ui_event_strategy() -> impl Strategy<Value = FuzzEvent> {
    let key = prop_oneof![
        Just(KeyCode::Up),
        Just(KeyCode::Down),
        Just(KeyCode::Left),
        Just(KeyCode::Right),
        Just(KeyCode::Home),
        Just(KeyCode::End),
        Just(KeyCode::PageUp),
        Just(KeyCode::PageDown),
        Just(KeyCode::Tab),
        Just(KeyCode::BackTab),
        Just(KeyCode::Char(' ')),
        Just(KeyCode::Char('a')),
        Just(KeyCode::Char('c')),
        Just(KeyCode::Char('e')),
        Just(KeyCode::Char('g')),
        Just(KeyCode::Char('i')),
        Just(KeyCode::Char('d')),
        Just(KeyCode::Char('n')),
        Just(KeyCode::Char('s')),
        Just(KeyCode::Char('?')),
        Just(KeyCode::Char('x')),
        Just(KeyCode::Char('y')),
        Just(KeyCode::Enter),
        Just(KeyCode::Esc),
    ];
    prop_oneof![
        7 => key.prop_map(FuzzEvent::Key),
        2 => any::<bool>().prop_map(FuzzEvent::Wheel),
        3 => (0u16..180u16, 0u16..50u16).prop_map(|(c, r)| FuzzEvent::Click(c, r)),
    ]
}

fn build_tree_and_contents(
    specs: &[(usize, FuzzNode)],
    pruned_count: usize,
) -> (DirectoryTree, HashMap<NodeId, String>) {
    let mut tree = DirectoryTree::new("fuzz_repo");
    let mut dir_ids = vec![DirectoryTree::ROOT];
    let mut contents = HashMap::new();

    for (idx, (parent_slot, kind)) in specs.iter().enumerate() {
        let parent = dir_ids[parent_slot % dir_ids.len()];
        match *kind {
            FuzzNode::Dir(expanded) => {
                let d = tree
                    .add_dir(parent, format!("dir_{idx}"), expanded)
                    .expect("dir");
                dir_ids.push(d);
            }
            FuzzNode::Text(nat, tokens, has_sec) => {
                let id = tree
                    .add_text_file(
                        parent,
                        format!("f_{idx}.rs"),
                        TextFileData {
                            byte_len: 16,
                            tokens,
                            secret_hits: usize::from(has_sec),
                            state: TextFileState::new(nat),
                        },
                    )
                    .expect("text");
                if has_sec {
                    tree.set_secret_findings(
                        id,
                        vec![SecretFinding {
                            line: 1,
                            rule: SecretRuleKind::AwsAccessKey,
                        }],
                    );
                }
                contents.insert(id, format!("// file {idx} <ok>\r\n"));
            }
            FuzzNode::Discarded => {
                tree.add_discarded_file(
                    parent,
                    format!("bin_{idx}.png"),
                    FileDiscardReason::Binary,
                )
                .expect("discarded");
            }
        }
    }
    for idx in 0..pruned_count {
        tree.record_pruned_dir(
            format!("dep_{idx}/vendor"),
            PrunedDirReason::DependencyOrVcs,
        );
    }
    (tree, contents)
}

fn assert_tui_invariants(state: &TuiAppState) -> Result<(), TestCaseError> {
    prop_assert!(!state.visible_rows().is_empty());
    prop_assert!(state.cursor() < state.visible_rows().len());
    prop_assert!(state.scroll_offset() <= state.cursor());
    prop_assert_eq!(state.metrics(), &state.tree().compute_metrics());
    prop_assert!(
        state.metrics().counts.active_secret_files <= state.metrics().counts.total_secret_files
    );
    Ok(())
}

fn run_fuzz_sequence(
    specs: &[(usize, FuzzNode)],
    pruned_count: usize,
    has_git: bool,
    viewport: (u16, u16),
    events: &[FuzzEvent],
) -> Result<(), TestCaseError> {
    let (tree, contents) = build_tree_and_contents(specs, pruned_count);
    let mut state = TuiAppState::new(tree, has_git);
    let mut layout = TuiViewportAreas::default();
    let mut terminal =
        Terminal::new(TestBackend::new(viewport.0, viewport.1)).expect("test backend");

    draw_screen_with_layout(&mut terminal, &mut state, &mut layout);
    assert_tui_invariants(&state)?;

    for &ev in events {
        match ev {
            FuzzEvent::Key(code) => {
                handle_key_event(&mut state, KeyEvent::new(code, KeyModifiers::NONE));
            }
            FuzzEvent::Wheel(down) => {
                let kind = if down {
                    MouseEventKind::ScrollDown
                } else {
                    MouseEventKind::ScrollUp
                };
                let _ = handle_mouse_event(
                    &mut state,
                    &layout,
                    MouseEvent {
                        kind,
                        column: 4,
                        row: 4,
                        modifiers: KeyModifiers::NONE,
                    },
                );
            }
            FuzzEvent::Click(c, r) => {
                let _ = handle_mouse_event(
                    &mut state,
                    &layout,
                    MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column: c % viewport.0,
                        row: r % viewport.1,
                        modifiers: KeyModifiers::NONE,
                    },
                );
            }
        }
        draw_screen_with_layout(&mut terminal, &mut state, &mut layout);
        assert_tui_invariants(&state)?;

        if let Some(exit) = state.exit_action() {
            if exit == AppExitAction::CopyToClipboard {
                let expected = state.metrics().counts.read_files;
                let xml = render_strict_xml(state.tree(), &contents, None).expect("xml");
                let parsed = parse_strict_xml(&xml).map_err(TestCaseError::fail)?;
                prop_assert_eq!(parsed.files.len(), expected);
            }
            break;
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn random_key_and_mouse_sequences_across_viewports_never_panic_or_corrupt_invariants(
        specs in prop::collection::vec(node_strategy(), 0..30),
        pruned_count in 0usize..4,
        has_git in any::<bool>(),
        viewport in prop_oneof![
            Just((30u16, 10u16)),
            Just((65u16, 16u16)),
            Just((80u16, 24u16)),
            Just((160u16, 40u16)),
        ],
        events in prop::collection::vec(ui_event_strategy(), 150..200),
    ) {
        run_fuzz_sequence(&specs, pruned_count, has_git, viewport, &events)?;
    }
}
