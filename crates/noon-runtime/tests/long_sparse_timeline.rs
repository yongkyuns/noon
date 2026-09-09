use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    CompositionTimeMap, Easing, GeometryRef, ObjectId, Property, Style, TrackDefinition, TrackId,
    TrackTiming, TrackValues, Transform2D, Vec2,
};
use noon_runtime::{EvaluationStats, SceneInstance};

const HISTORICAL_TRACKS: usize = 50_000;

#[test]
fn completed_long_timeline_history_is_not_rescanned_on_forward_frames() {
    let mut objects = Vec::new();
    let mut tracks = Vec::new();
    let object = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        object,
        GeometryRef::circle(0.5),
        Transform2D::IDENTITY,
        Style::default(),
    ));

    for index in 0..HISTORICAL_TRACKS {
        let x = index as f32;
        tracks.push(TrackDefinition {
            id: TrackId::new(tracks.len() as u64),
            object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::new(x, 0.0),
                to: Vec2::new(x + 1.0, 0.0),
            },
            timing: TrackTiming::new(index as f64, 0.5, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        });
    }

    let compiled = CompiledScene::compile_objects(objects, &tracks)
        .expect("long sparse timeline must compile");
    let mut runtime = SceneInstance::new(compiled);
    let history_end = HISTORICAL_TRACKS as f64;

    runtime
        .seek(history_end)
        .expect("seek past completed history must succeed");
    runtime.take_frame_changes();

    runtime
        .advance_to(history_end + 0.25)
        .expect("steady forward frame after history must succeed");

    assert_eq!(
        runtime.last_stats(),
        EvaluationStats::default(),
        "steady forward evaluation must perform no scheduler work after completed history"
    );
    assert!(
        runtime.take_frame_changes().is_empty(),
        "completed history must not republish unchanged object state"
    );
}
