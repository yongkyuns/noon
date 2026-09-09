//! Direct public-API counterpart of web/python/examples/live_affine_completion.py.
//! Construction, authored/effective queries, live edits and logical completion
//! need no raw store, compiler transaction, or host ownership knowledge.
use noon::{AnimationOptions, RateFunction, Scene, Vec2};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0)?;
    scene.add(&circle)?;
    let mut first_target = circle.target_editor()?;
    first_target.shift(2.0, -2.0)?;
    let mut second_target = circle.target_editor()?;
    second_target.shift(5.0, -2.0)?;
    let options = AnimationOptions::new()
        .run_time(2.0)
        .rate_func(RateFunction::Linear);
    let first = scene.declare_transform_to(&circle, &first_target, options)?;
    let second = scene.declare_transform_to(&circle, &second_target, options)?;
    let mut session = scene.execution_session()?;
    let mut live = scene.live(&mut session);
    let segment = live.play_animation(&first)?;
    live.advance_segment_to(segment, 1.0)?;
    assert_eq!(
        live.effective(&circle)?.transform.translation,
        Vec2::new(1.0, -1.0)
    );
    assert_eq!(live.authored(&circle)?.transform.translation.x, 0.0);
    live.advance_segment_to(segment, segment.end_time())?;
    assert!(!live.segment_state(segment).is_complete());
    live.complete_segment(segment)?;
    assert!(live.segment_state(segment).is_complete());
    assert_eq!(
        live.effective(&circle)?.transform.translation,
        Vec2::new(2.0, -2.0)
    );
    assert_eq!(live.authored(&circle)?.transform.translation.x, 2.0);
    live.set_translation(&circle, 3.0, -2.0)?;
    let wait = live.wait_segment(0.25)?;
    live.advance_segment_to(wait, wait.end_time())?;
    live.complete_segment(wait)?;
    assert_eq!(
        live.effective(&circle)?.transform.translation,
        Vec2::new(3.0, -2.0)
    );
    // Activation reads the edited effective value, not the declaration-time base.
    let segment = live.play_animation(&second)?;
    live.advance_segment_to(segment, segment.end_time() - 1.0)?;
    assert_eq!(
        live.effective(&circle)?.transform.translation,
        Vec2::new(4.0, -2.0)
    );
    assert_eq!(live.authored(&circle)?.transform.translation.x, 3.0);
    live.advance_segment_to(segment, segment.end_time())?;
    live.complete_segment(segment)?;
    assert_eq!(
        live.effective(&circle)?.transform.translation,
        Vec2::new(5.0, -2.0)
    );
    assert_eq!(live.authored(&circle)?.transform.translation.x, 5.0);
    assert!(live.segment_state(segment).is_complete());
    Ok(())
}
