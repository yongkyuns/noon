use noon_compile::CompiledScene;
use noon_core::{Easing, GeometryRef, SceneDefinition, TrackTiming, Vec2};
use noon_runtime::{EvaluationStats, SceneInstance};

const HISTORICAL_TRACKS: usize = 50_000;

#[test]
fn completed_long_timeline_history_is_not_rescanned_on_forward_frames() {
    let mut definition = SceneDefinition::new();
    let object = definition.add(GeometryRef::circle(0.5));

    for index in 0..HISTORICAL_TRACKS {
        let x = index as f32;
        definition
            .animate_position(
                object,
                Vec2::new(x, 0.0),
                Vec2::new(x + 1.0, 0.0),
                TrackTiming::new(index as f64, 0.5, Easing::Linear),
            )
            .expect("historical track must be valid");
    }

    let compiled = CompiledScene::compile(&definition).expect("long sparse timeline must compile");
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
