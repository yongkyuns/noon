use noon_compile::{CompiledObject, CompiledScene};
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

fn mapped_round_trip_runtime() -> SceneInstance {
    let object = ObjectId::new(1);
    let compiled = CompiledScene::compile_objects(
        vec![CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        )],
        &[
            mapped_appearance_track(1, object, 0.85, 1.8, 1.0, 0.0),
            mapped_appearance_track(2, object, 3.75, 1.8, 0.0, 1.0),
        ],
    )
    .unwrap();
    SceneInstance::new(compiled)
}

#[test]
fn later_mapped_appearance_track_owns_its_exact_endpoint() {
    let mut forward = mapped_round_trip_runtime();
    forward.advance_to(2.65).unwrap();
    assert_eq!(forward.frame().objects[0].appearance, 0.0);

    forward.advance_to(4.65).unwrap();
    let midpoint = forward.frame().objects[0].appearance;
    assert!(midpoint > 0.0 && midpoint < 1.0, "midpoint={midpoint}");

    forward.advance_to(5.55).unwrap();
    assert_eq!(forward.frame().objects[0].appearance, 1.0);

    let mut direct = mapped_round_trip_runtime();
    direct.seek(5.55).unwrap();
    assert_eq!(direct.frame().objects[0].appearance, 1.0);
}
