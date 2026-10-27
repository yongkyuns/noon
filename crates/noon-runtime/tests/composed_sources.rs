//! Typed native semantic lowering and the existing atomic runtime publication.
use noon_compile::{
    lower_semantic_execution, CompilePatchError, ExecutionMutationTransaction, ExecutionPatch,
    SemanticExecutionIndex,
};
use noon_core::{
    ComposedSource, EffectTrackEndBehavior, EffectTrackValue, EffectUpdate, Glow, ObjectContentRef,
    ObjectId, Property, RateFunction, SemanticMutationTransaction, SemanticNodeId,
    SemanticObjectState, SemanticStore, StoredGeometry, TrackDefinition, TrackId, TrackTiming,
    TrackValues,
};
use noon_runtime::SceneInstance;
use std::sync::Arc;

fn fixture() -> (SceneInstance, [ObjectId; 3], ObjectId, SemanticNodeId) {
    let mut store = SemanticStore::new();
    let nodes = [0.0, 2.0, 3.0].map(|z| {
        let mut value = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.5 });
        value.set_z_index(z);
        store.insert_semantic_object(value)
    });
    let owner = store.insert_family();
    for &child in &nodes[..2] {
        store.add_member(owner, child).unwrap();
    }
    let mut tx = SemanticMutationTransaction::new();
    let pending = tx.create_effect(
        owner,
        "glow",
        Glow::new(EffectUpdate::default().intensity(0.25)).unwrap(),
    );
    let attachment = tx.apply(&mut store).unwrap().resolve(pending).unwrap();
    store.attach_to_scene(owner).unwrap();
    store.attach_to_scene(nodes[2]).unwrap();
    let mut index = SemanticExecutionIndex::new();
    let output = lower_semantic_execution(&store, &mut index).unwrap();
    (
        SceneInstance::from_semantic_execution(output),
        nodes.map(|node| index.execution_object_id(node).unwrap()),
        index.execution_object_id(owner).unwrap(),
        attachment,
    )
}
fn plan(owner: ObjectId, members: impl IntoIterator<Item = ObjectId>) -> ExecutionPatch {
    ExecutionPatch::SetContent {
        object: owner,
        content: ObjectContentRef::Composed(Arc::new(ComposedSource::new(members, None).unwrap())),
        text_bounds: None,
    }
}
fn intensity(runtime: &SceneInstance, owner: ObjectId, attachment: SemanticNodeId) -> f64 {
    runtime.frame().objects[runtime.frame_index_for_object(owner).unwrap()]
        .effects
        .as_ref()
        .unwrap()
        .get(attachment)
        .unwrap()
        .definition
        .as_glow()
        .unwrap()
        .intensity()
}

#[test]
fn invalid_interleaving_and_removal_preserve_frame_publication_and_active_channels() {
    let (mut runtime, [a, b, outside], owner, attachment) = fixture();
    runtime
        .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
            id: TrackId::new(77),
            object: owner,
            property: Property::EffectIntensity,
            values: TrackValues::Effect {
                attachment,
                from: EffectTrackValue::Intensity(0.25),
                to: EffectTrackValue::Intensity(2.25),
                end_behavior: EffectTrackEndBehavior::Sample,
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: Default::default(),
        }))
        .unwrap();
    runtime.seek(1.0).unwrap();
    runtime.take_frame_changes();
    runtime.take_spatial_changes();
    let before = runtime.frame().clone();
    let context = runtime.publication_context();
    for patches in [
        vec![ExecutionPatch::SetZIndex {
            object: outside,
            value: 1.0,
        }],
        vec![ExecutionPatch::RemoveObject(a)],
    ] {
        assert!(matches!(
            runtime.apply_execution_transaction(&ExecutionMutationTransaction::from_mutations(
                patches
            )),
            Err(CompilePatchError::InvalidComposedSource(_))
        ));
        assert_eq!(runtime.frame(), &before);
        assert_eq!(runtime.publication_context(), context);
        assert!(runtime.take_frame_changes().is_empty());
    }
    runtime
        .apply_execution_transaction(&ExecutionMutationTransaction::from_mutations([
            plan(owner, [a, outside, b]),
            ExecutionPatch::SetZIndex {
                object: outside,
                value: 1.0,
            },
        ]))
        .unwrap();
    assert_eq!(runtime.frame().time, 1.0);
    assert_eq!(intensity(&runtime, owner, attachment), 1.25);
    assert_eq!(
        runtime
            .painter_order()
            .iter()
            .map(|&index| runtime.frame().objects[index as usize].id)
            .collect::<Vec<_>>(),
        [a, outside, b, owner]
    );
    runtime.seek(1.5).unwrap();
    assert_eq!(intensity(&runtime, owner, attachment), 1.75);
    let a_index = runtime.frame_index_for_object(a).unwrap();
    runtime
        .apply_execution_transaction(&ExecutionMutationTransaction::from_mutations([
            ExecutionPatch::RemoveObject(a),
            plan(owner, [outside, b]),
        ]))
        .unwrap();
    assert!(!runtime.frame().is_present(a_index));
    assert_eq!(runtime.frame().objects[a_index].id, a);
    assert_eq!(intensity(&runtime, owner, attachment), 1.75);
}

#[test]
fn controller_metadata_stays_after_drawable_painter_rows_across_seek_and_live_priority() {
    let (mut runtime, [a, b, outside], owner, _) = fixture();
    for time in [0.0, 1.0, 0.0] {
        runtime.seek(time).unwrap();
        assert_eq!(
            runtime
                .painter_order()
                .iter()
                .map(|&index| runtime.frame().objects[index as usize].id)
                .collect::<Vec<_>>(),
            [a, b, outside, owner]
        );
    }
    runtime
        .apply_execution_patch(&ExecutionPatch::SetZIndex {
            object: owner,
            value: -20.0,
        })
        .unwrap();
    assert_eq!(
        runtime
            .painter_order()
            .last()
            .map(|&index| runtime.frame().objects[index as usize].id),
        Some(owner)
    );
}
