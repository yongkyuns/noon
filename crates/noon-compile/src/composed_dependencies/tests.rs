use super::*;
use crate::{CompileError, CompilePatchError, CompiledObject, ExecutionMutationTransaction};
use noon_core::{GeometryRef, ObjectContentRef, Style, Transform2D};
use std::sync::Arc;

fn id(value: u64) -> ObjectId {
    ObjectId::new(value)
}
fn leaf(value: u64, z: f64) -> CompiledObject {
    let mut row = CompiledObject::new(
        id(value),
        GeometryRef::circle(0.5),
        Transform2D::IDENTITY,
        Style::default(),
    );
    row.base_z_index = z;
    row
}
fn content(members: &[u64], parent: Option<u64>) -> ObjectContentRef {
    ObjectContentRef::Composed(Arc::new(
        ComposedSource::new(members.iter().copied().map(id), parent.map(id)).unwrap(),
    ))
}
fn scope(value: u64, members: &[u64], parent: Option<u64>) -> CompiledObject {
    let mut row = CompiledObject::new(
        id(value),
        content(members, parent),
        Transform2D::IDENTITY,
        Style {
            fill: None,
            stroke: None,
            ..Style::default()
        },
    );
    row.draw_enabled = false;
    row
}
fn fixture() -> CompiledScene {
    CompiledScene::compile_objects(
        vec![
            leaf(1, 0.0),
            leaf(2, 2.0),
            leaf(3, 3.0),
            scope(9, &[1, 2], None),
        ],
        &[],
    )
    .unwrap()
}
fn plan(members: &[u64]) -> ExecutionPatch {
    ExecutionPatch::SetContent {
        object: id(9),
        content: content(members, None),
        text_bounds: None,
    }
}
fn commit(scene: &mut CompiledScene, patches: Vec<ExecutionPatch>) -> ComposedValidationStats {
    let tx = ExecutionMutationTransaction::from_mutations(patches);
    let stats = scene.preflight_execution_transaction(&tx).unwrap();
    for patch in tx.mutations() {
        scene
            .commit_preflighted_execution_patch_with_stats(patch)
            .unwrap();
    }
    stats.composed
}

#[test]
fn initial_nested_equal_intervals_resolve_future_parent_and_full_input_generations() {
    let scene = CompiledScene::compile_objects(
        vec![
            scope(9, &[1], Some(10)),
            leaf(1, 0.0),
            scope(10, &[1], None),
        ],
        &[],
    )
    .unwrap();
    assert_eq!(
        scene
            .composed_dependencies
            .dependents(id(1))
            .collect::<Vec<_>>(),
        [id(9), id(10)]
    );
    assert_eq!(
        scene.composed_dependencies.children[&id(10)],
        BTreeSet::from([id(9)])
    );
    let wrong = 1_u64 << 32 | 1;
    assert!(
        matches!(CompiledScene::compile_objects(vec![leaf(1,0.0),scope(9,&[wrong],None)], &[]),
        Err(CompileError::InvalidComposedSource(CompiledComposedSourceError { kind: CompiledComposedSourceErrorKind::UnknownInput(input),.. })) if input == id(wrong))
    );
}

#[test]
fn raw_compilation_rejects_invalid_references_cycles_order_and_ambiguous_containment() {
    let cases = [
        vec![leaf(1, 0.0), scope(9, &[1], Some(99))],
        vec![
            leaf(1, 0.0),
            scope(9, &[1], Some(10)),
            scope(10, &[1], Some(9)),
        ],
        vec![leaf(1, 0.0), scope(9, &[1], None), scope(10, &[1], None)],
        vec![leaf(1, 0.0), scope(9, &[1], None), scope(10, &[9], None)],
        vec![
            leaf(1, 0.0),
            leaf(2, 1.0),
            leaf(3, 2.0),
            scope(9, &[1, 3], None),
        ],
        vec![leaf(1, 0.0), leaf(2, 1.0), scope(9, &[2, 1], None)],
        vec![
            leaf(1, 0.0),
            leaf(2, 1.0),
            scope(9, &[1], Some(10)),
            scope(10, &[2], None),
        ],
        vec![
            leaf(1, 0.0),
            leaf(2, 1.0),
            leaf(3, 2.0),
            scope(9, &[1, 2], None),
            scope(10, &[2, 3], None),
        ],
    ];
    for (case, rows) in cases.into_iter().enumerate() {
        assert!(
            matches!(
                CompiledScene::compile_objects(rows, &[]),
                Err(CompileError::InvalidComposedSource(_))
            ),
            "invalid composed graph {case}"
        );
    }
    let mut drawable_controller = scope(9, &[1], None);
    drawable_controller.draw_enabled = true;
    assert!(matches!(
        CompiledScene::compile_objects(vec![leaf(1, 0.0), drawable_controller], &[]),
        Err(CompileError::InvalidComposedSource(_))
    ));
}

#[test]
fn single_removal_and_outside_interleaving_reject_before_any_compiled_write() {
    let mut scene = fixture();
    let before = scene.clone();
    for patch in [
        ExecutionPatch::RemoveObject(id(1)),
        ExecutionPatch::SetZIndex {
            object: id(3),
            value: 1.0,
        },
        plan(&[1, 3]),
    ] {
        assert!(matches!(
            scene.apply_execution_patch(&patch),
            Err(CompilePatchError::InvalidComposedSource(_))
        ));
        assert_eq!(scene, before);
    }
}

#[test]
fn complete_order_and_plan_edits_validate_final_state_in_either_patch_order() {
    for reverse in [false, true] {
        let mut scene = fixture();
        let mut edits = vec![
            ExecutionPatch::SetZIndex {
                object: id(3),
                value: 1.0,
            },
            plan(&[1, 3, 2]),
        ];
        if reverse {
            edits.reverse();
        }
        let stats = commit(&mut scene, edits);
        assert_eq!(
            scene
                .painter_order()
                .iter()
                .map(|&index| scene.objects()[index as usize].id)
                .collect::<Vec<_>>(),
            [id(1), id(3), id(2), id(9)]
        );
        assert_eq!(stats.scopes_validated, 1);
        assert_eq!(stats.members_examined, 3);
        assert_eq!(stats.painter_rows_staged, 2);
        assert_eq!(
            scene
                .composed_dependencies
                .dependents(id(3))
                .collect::<Vec<_>>(),
            [id(9)]
        );
    }
}

#[test]
fn atomic_removal_updates_reverse_inputs_and_allows_controller_before_new_members() {
    for reverse in [false, true] {
        let mut scene = fixture();
        let mut edits = vec![ExecutionPatch::RemoveObject(id(1)), plan(&[2])];
        if reverse {
            edits.reverse();
        }
        commit(&mut scene, edits);
        assert_eq!(scene.composed_dependencies.dependents(id(1)).count(), 0);
        scene
            .apply_execution_patch(&ExecutionPatch::RemoveObject(id(3)))
            .unwrap();
        assert!(scene
            .apply_execution_patch(&ExecutionPatch::RemoveObject(id(2)))
            .is_err());
    }
    let mut scene = CompiledScene::compile_objects(vec![], &[]).unwrap();
    commit(
        &mut scene,
        vec![
            ExecutionPatch::CreateObject(scope(9, &[1, 2], Some(10))),
            ExecutionPatch::CreateObject(scope(10, &[1, 2], None)),
            ExecutionPatch::CreateObject(leaf(1, 0.0)),
            ExecutionPatch::CreateObject(leaf(2, 0.0)),
        ],
    );
    assert_eq!(scene.composed_dependencies.dependents(id(2)).count(), 2);
}

#[test]
fn parameter_and_unrelated_edits_do_not_walk_scope_members_or_scene_payloads() {
    let mut rows = (1..=2048).map(|i| leaf(i, 0.0)).collect::<Vec<_>>();
    rows.push(scope(9000, &(1..=512).collect::<Vec<_>>(), None));
    let scene = CompiledScene::compile_objects(rows, &[]).unwrap();
    for object in [id(9000), id(2048)] {
        let tx = ExecutionMutationTransaction::from_mutations([ExecutionPatch::SetStyle {
            object,
            style: Style {
                fill: None,
                stroke: None,
                opacity: 0.5,
                ..Style::default()
            },
            fill_paint: None,
        }]);
        let stats = scene.preflight_execution_transaction(&tx).unwrap();
        assert_eq!(stats.composed, ComposedValidationStats::default());
        assert_eq!(stats.staged_compiled_scene_clones, 0);
        assert_eq!(stats.objects_indexed, 1);
    }
    let stats = scene
        .preflight_execution_transaction(&ExecutionMutationTransaction::from_mutations([
            ExecutionPatch::RemoveObject(id(2048)),
        ]))
        .unwrap();
    assert_eq!(stats.composed.scopes_validated, 0);
    assert_eq!(stats.composed.members_examined, 0);
    assert_eq!(
        stats.composed.painter_rows_staged, 1,
        "only trailing metadata moves"
    );
}

#[test]
fn replay_and_retired_row_compaction_preserve_exact_input_dependencies() {
    let mut scene = fixture();
    let patch = plan(&[1]);
    let mut revision = scene.prepare_replay_revision(&patch).unwrap();
    scene.apply_execution_patch(&patch).unwrap();
    assert_eq!(scene.composed_dependencies.dependents(id(2)).count(), 0);
    scene.exchange_replay_revision(&mut revision);
    assert_eq!(
        scene
            .composed_dependencies
            .dependents(id(2))
            .collect::<Vec<_>>(),
        [id(9)]
    );
    scene.exchange_replay_revision(&mut revision);
    assert_eq!(scene.composed_dependencies.dependents(id(2)).count(), 0);
    scene
        .apply_execution_patch(&ExecutionPatch::RemoveObject(id(2)))
        .unwrap();
    scene.compact_retired_object_slots().unwrap();
    assert_eq!(
        scene
            .composed_dependencies
            .dependents(id(1))
            .collect::<Vec<_>>(),
        [id(9)]
    );
    assert!(scene
        .apply_execution_patch(&ExecutionPatch::RemoveObject(id(1)))
        .is_err());
}

#[test]
fn nested_reparenting_and_parent_retirement_validate_one_coherent_transaction() {
    let mut scene = CompiledScene::compile_objects(
        vec![
            leaf(1, 0.0),
            scope(9, &[1], Some(10)),
            scope(10, &[1], None),
        ],
        &[],
    )
    .unwrap();
    commit(
        &mut scene,
        vec![
            ExecutionPatch::SetContent {
                object: id(10),
                content: content(&[1], Some(11)),
                text_bounds: None,
            },
            ExecutionPatch::CreateObject(scope(11, &[1], None)),
        ],
    );
    let before = scene.clone();
    assert!(scene
        .apply_execution_patch(&ExecutionPatch::RemoveObject(id(11)))
        .is_err());
    assert_eq!(scene, before);
    commit(
        &mut scene,
        vec![
            ExecutionPatch::RemoveObject(id(11)),
            ExecutionPatch::SetContent {
                object: id(10),
                content: content(&[1], None),
                text_bounds: None,
            },
        ],
    );
    assert!(!scene.composed_dependencies.children.contains_key(&id(11)));
    assert_eq!(
        scene.composed_dependencies.children[&id(10)],
        BTreeSet::from([id(9)])
    );
}

#[test]
fn source_generation_churn_retires_reverse_edges_and_compacts_without_rebinding() {
    let mut scene = fixture();
    let mut input = id(1);
    for generation in 1..=128_u64 {
        let next = generation << 32 | 1;
        commit(
            &mut scene,
            vec![
                ExecutionPatch::RemoveObject(input),
                ExecutionPatch::CreateObject(leaf(next, 0.0)),
                plan(&[next, 2]),
            ],
        );
        assert_eq!(scene.composed_dependencies.dependents(input).count(), 0);
        assert_eq!(scene.composed_dependencies.members.len(), 2);
        input = id(next);
    }
    let stats = scene.compact_retired_object_slots().unwrap();
    assert_eq!(stats.object_slots_reclaimed, 128);
    assert_eq!(
        scene
            .composed_dependencies
            .dependents(input)
            .collect::<Vec<_>>(),
        [id(9)]
    );
    scene
        .apply_execution_patch(&ExecutionPatch::RemoveObject(id(9)))
        .unwrap();
    assert_eq!(scene.composed_dependencies, ComposedDependencies::default());
}
