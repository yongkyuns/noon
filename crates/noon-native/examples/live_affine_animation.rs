//! Direct Rust proof for one replayable affine declaration and live continuation.
//! No scene document, legacy geometry materialization, or frontend scheduler is used.
use noon::{AnimationOptions, RateFunction, Scene, Vec2};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0)?;
    scene.add(&circle)?;

    // The target and declaration are shared semantic authoring performed before
    // an execution session exists. They remain replayable scene meaning.
    let mut target = circle.target_editor()?;
    target.shift(4.0, -2.0)?;
    target.manim_scale(2.0, 2.0)?;
    let animation = scene.declare_transform_to(
        &circle,
        &target,
        AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    )?;

    let mut session = scene.execution_session()?;
    let mut live = scene.live(&mut session);
    let segment = live.play_animation(&animation)?;
    live.advance_segment_to(segment, 1.0)?;
    assert_eq!(
        live.effective(&circle)?.transform.translation,
        Vec2::new(2.0, -1.0)
    );
    assert!(!live.segment_state(segment).is_complete());

    live.advance_segment_to(segment, segment.end_time())?;
    assert!(!live.segment_state(segment).is_complete());
    live.complete_segment(segment)?;
    assert!(live.segment_state(segment).is_complete());
    assert_eq!(
        live.effective(&circle)?.transform.translation,
        Vec2::new(4.0, -2.0)
    );

    // Wait uses the same runtime continuation boundary and allocates no track.
    let wait = live.wait_segment(0.25)?;
    live.advance_segment_to(wait, wait.end_time())?;
    live.complete_segment(wait)?;
    assert!(live.segment_state(wait).is_complete());
    noon_native::run(session)?;
    Ok(())
}
