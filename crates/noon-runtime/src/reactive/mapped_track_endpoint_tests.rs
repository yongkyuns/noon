use noon_compile::{
    lower_semantic_execution, ExecutionMutationTransaction, ExecutionPatch, SemanticExecutionIndex,
};
use noon_core::{
    CompositionTimeMap, CompositionTimeMapStep, ObjectId, Property, RateFunction,
    SemanticObjectState, SemanticStore, StoredGeometry, TrackDefinition, TrackId, TrackTiming,
    TrackValues,
};

use crate::SceneInstance;

fn mapped_appearance_track(
    id: u64,
    object: ObjectId,
    start_time: f64,
    duration: f64,
    from: f32,
    to: f32,
) -> TrackDefinition {
    TrackDefinition {
        id: TrackId::new(id),
        object,
        property: Property::Appearance,
        values: TrackValues::Scalar { from, to },
        timing: TrackTiming::new(start_time, duration, RateFunction::Smooth),
        time_map: CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
            0.0,
            1.0,
            RateFunction::Linear,
        )]),
    }
}

fn retained_hide_runtime() -> (SceneInstance, ObjectId) {
    let mut store = SemanticStore::new();
    let node = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(node).unwrap();
    let mut index = SemanticExecutionIndex::new();
    let output = lower_semantic_execution(&store, &mut index).unwrap();
    let object = index.execution_object_id(node).unwrap();
    let (mut compiled, _reactive) = output.into_parts();
    compiled
        .apply_execution_patch(&ExecutionPatch::AddTrack(mapped_appearance_track(
            1, object, 0.85, 1.8, 1.0, 0.0,
        )))
        .unwrap();
    (SceneInstance::new(compiled), object)
}

fn install_restoration(runtime: &mut SceneInstance, object: ObjectId) {
    let transaction = ExecutionMutationTransaction::from_mutations([
        ExecutionPatch::AddTrack(mapped_appearance_track(2, object, 3.75, 1.8, 0.0, 1.0)),
    ]);
    runtime.apply_execution_transaction(&transaction).unwrap();
}

#[test]
fn live_added_mapped_appearance_track_owns_its_exact_endpoint() {
    let (mut forward, object) = retained_hide_runtime();
    forward.advance_to(2.65).unwrap();
    assert_eq!(forward.frame().objects[0].appearance, 0.0);
    forward.advance_to(3.75).unwrap();
    install_restoration(&mut forward, object);

    forward.advance_to(4.65).unwrap();
    let midpoint = forward.frame().objects[0].appearance;
    assert!(midpoint > 0.0 && midpoint < 1.0, "midpoint={midpoint}");

    forward.advance_to(5.55).unwrap();
    assert_eq!(forward.frame().objects[0].appearance, 1.0);

    let (mut direct, object) = retained_hide_runtime();
    direct.advance_to(3.75).unwrap();
    install_restoration(&mut direct, object);
    direct.seek(5.55).unwrap();
    assert_eq!(direct.frame().objects[0].appearance, 1.0);
}
