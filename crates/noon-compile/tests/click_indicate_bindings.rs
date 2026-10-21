use std::sync::Arc;

use noon_compile::{
    lower_semantic_execution, prepare_semantic_publication, CompileError, CompilePatchError,
    CompiledObject, CompiledScene, ExecutionMutationTransaction, ExecutionPatch,
    SemanticExecutionIndex, SemanticExecutionReachability,
};
use noon_core::{
    Color, GeometryRef, ObjectId, SemanticClickIndicate, SemanticMutationTransaction,
    SemanticObjectProperty, SemanticObjectState, SemanticPaint, SemanticStore, StoredGeometry,
    Style, Transform2D, Vec2, VectorPath,
};

fn binding(color: Color) -> SemanticClickIndicate {
    SemanticClickIndicate::new(1.5, color, 2.0)
}

fn object(id: u64, action: Option<SemanticClickIndicate>) -> CompiledObject {
    let mut object = CompiledObject::new(
        ObjectId::new(id),
        GeometryRef::circle(0.5),
        Transform2D::IDENTITY,
        Style::default(),
    );
    object.click_indicate = action.map(Arc::new);
    object
}

fn path() -> GeometryRef {
    GeometryRef::path(
        VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(1.0, 0.0))
            .line_to(Vec2::new(0.0, 1.0))
            .close(),
    )
}

#[test]
fn semantic_lowering_retains_the_action_and_existing_outline_declaration_policy() {
    let mut store = SemanticStore::new();
    let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.5 });
    state.style.fill = None;
    state.style.stroke = Some(SemanticPaint::Solid(Color::BLUE));
    state.set_click_indicate(Some(binding(Color::YELLOW)));
    let node = store.insert_semantic_object(state);
    store.attach_to_scene(node).unwrap();
    let mut index = SemanticExecutionIndex::new();
    let (compiled, _) = lower_semantic_execution(&store, &mut index)
        .unwrap()
        .into_parts();
    let id = index.execution_object_id(node).unwrap();
    assert_eq!(
        compiled.click_indicate_binding(id),
        Some(binding(Color::YELLOW))
    );
    assert!(compiled.has_click_indicate_bindings());
    assert_eq!(compiled.objects()[0].base_style.fill, None);
}

#[test]
fn invalid_initial_semantic_actions_do_not_publish_identity() {
    for (geometry, action, driven) in [
        (
            StoredGeometry::Circle { radius: 0.5 },
            SemanticClickIndicate::new(1.5, Color::YELLOW, 0.0),
            false,
        ),
        (
            StoredGeometry::Line {
                start: Vec2::ZERO,
                end: Vec2::ONE,
            },
            binding(Color::YELLOW),
            false,
        ),
        (
            StoredGeometry::Circle { radius: 0.5 },
            binding(Color::YELLOW),
            true,
        ),
    ] {
        let mut store = SemanticStore::new();
        let mut state = SemanticObjectState::new(geometry);
        state.set_click_indicate(Some(action));
        let node = store.insert_semantic_object(state);
        store.attach_to_scene(node).unwrap();
        if driven {
            let signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
            store
                .bind_semantic_signal(signal, node, SemanticObjectProperty::ObjectOpacity)
                .unwrap();
        }
        let mut index = SemanticExecutionIndex::new();
        assert!(lower_semantic_execution(&store, &mut index).is_err());
        assert!(index.is_empty());
    }
}

#[test]
fn invalid_compiled_actions_and_content_changes_preserve_the_projection() {
    let id = ObjectId::new(0);
    let mut compiled =
        CompiledScene::compile_objects(vec![object(0, Some(binding(Color::YELLOW)))], &[]).unwrap();
    let previous = compiled.clone();
    for action in [
        SemanticClickIndicate::new(0.0, Color::YELLOW, 1.0),
        SemanticClickIndicate::new(1.5, Color::YELLOW, f64::NAN),
        SemanticClickIndicate::new(
            1.5,
            Color {
                red: 2.0,
                ..Color::YELLOW
            },
            1.0,
        ),
    ] {
        assert!(matches!(
            compiled.apply_execution_patch(&ExecutionPatch::SetClickIndicate {
                object: id,
                binding: Some(action),
            }),
            Err(CompilePatchError::InvalidClickIndicate(rejected)) if rejected == id
        ));
        assert_eq!(compiled, previous);
        assert!(matches!(
            CompiledScene::compile_objects(vec![object(1, Some(action))], &[]),
            Err(CompileError::InvalidClickIndicate(_))
        ));
    }
    for geometry in [path(), GeometryRef::line(Vec2::ZERO, Vec2::ONE)] {
        assert!(matches!(
            compiled.apply_execution_patch(&ExecutionPatch::SetContent {
                object: id,
                content: geometry.clone().into(),
                text_bounds: None,
            }),
            Err(CompilePatchError::InvalidClickIndicate(rejected)) if rejected == id
        ));
        let mut unsupported = object(1, Some(binding(Color::YELLOW)));
        unsupported.content = geometry.into();
        assert!(matches!(
            compiled.apply_execution_patch(&ExecutionPatch::CreateObject(unsupported)),
            Err(CompilePatchError::InvalidClickIndicate(_))
        ));
        assert_eq!(compiled, previous);
    }
}

#[test]
fn sparse_preflight_tracks_binding_and_content_changes_before_commit() {
    let id = ObjectId::new(0);
    let mut compiled = CompiledScene::compile_objects(
        (0..10_000)
            .map(|id| object(id, (id == 0).then(|| binding(Color::YELLOW))))
            .collect(),
        &[],
    )
    .unwrap();
    let clear_and_replace = ExecutionMutationTransaction::from_mutations([
        ExecutionPatch::SetClickIndicate {
            object: id,
            binding: None,
        },
        ExecutionPatch::SetContent {
            object: id,
            content: path().into(),
            text_bounds: None,
        },
    ]);
    let stats = compiled
        .preflight_execution_transaction(&clear_and_replace)
        .unwrap();
    assert_eq!(stats.objects_indexed, 1);
    assert_eq!(stats.tracks_indexed, 0);
    assert_eq!(stats.staged_compiled_scene_clones, 0);
    let previous = compiled.clone();
    let mut rejected = clear_and_replace.clone();
    rejected.push(ExecutionPatch::SetClickIndicate {
        object: id,
        binding: Some(binding(Color::RED)),
    });
    assert_eq!(
        compiled.preflight_execution_transaction(&rejected),
        Err(CompilePatchError::InvalidClickIndicate(id))
    );
    assert_eq!(compiled, previous);
    for patch in clear_and_replace.mutations() {
        compiled.apply_execution_patch(patch).unwrap();
    }
    assert!(!compiled.has_click_indicate_bindings());

    let restore = ExecutionMutationTransaction::from_mutations([
        ExecutionPatch::SetContent {
            object: id,
            content: GeometryRef::rectangle(1.0, 0.5).into(),
            text_bounds: None,
        },
        ExecutionPatch::SetClickIndicate {
            object: id,
            binding: Some(binding(Color::RED)),
        },
    ]);
    compiled.preflight_execution_transaction(&restore).unwrap();
    for patch in restore.mutations() {
        compiled.apply_execution_patch(patch).unwrap();
    }
    assert_eq!(
        compiled.click_indicate_binding(id),
        Some(binding(Color::RED))
    );
    assert!(compiled.has_click_indicate_bindings());
}

#[test]
fn semantic_clear_and_content_replacement_lower_in_a_valid_execution_order() {
    let mut store = SemanticStore::new();
    let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.5 });
    state.set_click_indicate(Some(binding(Color::YELLOW)));
    let node = store.insert_semantic_object(state);
    store.attach_to_scene(node).unwrap();
    let mut index = SemanticExecutionIndex::new();
    let (compiled, _) = lower_semantic_execution(&store, &mut index)
        .unwrap()
        .into_parts();
    let reachability = SemanticExecutionReachability::from_store(&store).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_click_indicate(node, None);
    transaction.replace_content(
        node,
        StoredGeometry::Line {
            start: Vec2::ZERO,
            end: Vec2::ONE,
        },
    );
    let prepared = transaction.prepare(&mut store).unwrap();
    let publication = prepare_semantic_publication(&prepared, &index, &reachability).unwrap();
    assert!(matches!(
        publication.value_transaction().mutations(),
        [
            ExecutionPatch::SetClickIndicate { binding: None, .. },
            ExecutionPatch::SetContent { .. }
        ]
    ));
    compiled
        .preflight_execution_transaction(publication.value_transaction())
        .unwrap();
    assert!(compiled.has_click_indicate_bindings());
}

#[test]
fn ordinary_row_revisions_restore_binding_interest_through_changes_and_removal() {
    let id = ObjectId::new(0);
    let mut compiled = CompiledScene::compile_objects(vec![object(0, None)], &[]).unwrap();
    let mut revisions = Vec::new();
    for patch in [
        ExecutionPatch::SetClickIndicate {
            object: id,
            binding: Some(binding(Color::YELLOW)),
        },
        ExecutionPatch::SetClickIndicate {
            object: id,
            binding: Some(binding(Color::RED)),
        },
        ExecutionPatch::SetClickIndicate {
            object: id,
            binding: None,
        },
        ExecutionPatch::RemoveObject(id),
    ] {
        let revision = compiled.prepare_replay_revision(&patch).unwrap();
        assert_eq!(revision.retention_cost(), 1);
        assert_eq!(revision.object_indices().collect::<Vec<_>>(), vec![0]);
        compiled.apply_execution_patch(&patch).unwrap();
        revisions.push(revision);
    }
    assert!(!compiled.has_click_indicate_bindings());
    let expected = [
        None,
        Some(binding(Color::RED)),
        Some(binding(Color::YELLOW)),
        None,
    ];
    for (revision, action) in revisions.iter_mut().rev().zip(expected) {
        compiled.exchange_replay_revision(revision);
        assert_eq!(compiled.click_indicate_binding(id), action);
        assert_eq!(compiled.has_click_indicate_bindings(), action.is_some());
    }
    let expected = [
        Some(binding(Color::YELLOW)),
        Some(binding(Color::RED)),
        None,
        None,
    ];
    for (revision, action) in revisions.iter_mut().zip(expected) {
        compiled.exchange_replay_revision(revision);
        assert_eq!(compiled.click_indicate_binding(id), action);
        assert_eq!(compiled.has_click_indicate_bindings(), action.is_some());
    }
    compiled
        .apply_execution_patch(&ExecutionPatch::CreateObject(object(
            0,
            Some(binding(Color::YELLOW)),
        )))
        .unwrap();
    assert!(compiled.has_click_indicate_bindings());
    compiled
        .apply_execution_patch(&ExecutionPatch::CreateObject(object(
            1,
            Some(binding(Color::RED)),
        )))
        .unwrap();
    compiled
        .apply_execution_patch(&ExecutionPatch::RemoveObject(id))
        .unwrap();
    assert!(compiled.has_click_indicate_bindings());
    compiled.compact_retired_object_slots().unwrap();
    assert_eq!(
        compiled.click_indicate_binding(ObjectId::new(1)),
        Some(binding(Color::RED))
    );
    compiled
        .apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(1)))
        .unwrap();
    assert!(!compiled.has_click_indicate_bindings());
}

#[cfg(feature = "prepared-prefix")]
#[test]
fn appended_tombstones_do_not_create_pointer_interest() {
    let mut compiled = CompiledScene::compile_objects(vec![object(0, None)], &[]).unwrap();
    compiled.reserve_prefix_object_capacity(2).unwrap();
    compiled.install_prefix_tombstones(vec![object(1, Some(binding(Color::YELLOW)))]);
    assert_eq!(compiled.click_indicate_binding(ObjectId::new(1)), None);
    assert!(!compiled.has_click_indicate_bindings());
}
