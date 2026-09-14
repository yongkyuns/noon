use noon_compile::{CompiledObject, CompiledScene, ExecutionPatch};
use noon_core::{
    CompositionTimeMap, CompositionTimeMapStep, GeometryRef, ObjectId, Property, RateFunction,
    Style, TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D,
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
    let object = ObjectId::new(1);
    let compiled = CompiledScene::compile_objects(
        vec![CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        )],
        &[mapped_appearance_track(1, object, 0.85, 1.8, 1.0, 0.0)],
    )
    .unwrap();
    (SceneInstance::new(compiled), object)
}

fn install_restoration(runtime: &mut SceneInstance, object: ObjectId) {
    runtime
        .apply_execution_patch(&ExecutionPatch::AddTrack(mapped_appearance_track(
            2, object, 3.75, 1.8, 0.0, 1.0,
        )))
        .unwrap();
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
