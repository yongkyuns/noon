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
    let mut session = scene.into_execution()?;
    let segment = session.live().play_animation(&first)?;
    session.live().advance_segment_to(segment, 1.0)?;
    assert_eq!(
        session.live().effective(&circle)?.transform.translation,
        Vec2::new(1.0, -1.0)
    );
    assert_eq!(
        session.live().authored(&circle)?.transform.translation.x,
        0.0
    );
    session
        .live()
        .advance_segment_to(segment, segment.end_time())?;
    assert!(!session.live().segment_state(segment).is_complete());
    session.live().complete_segment(segment)?;
    assert!(session.live().segment_state(segment).is_complete());
    assert_eq!(
        session.live().effective(&circle)?.transform.translation,
        Vec2::new(2.0, -2.0)
    );
    assert_eq!(
        session.live().authored(&circle)?.transform.translation.x,
        2.0
    );
    session.scene_mut().set_translation(&circle, 3.0, -2.0)?;
    let wait = session.live().wait_segment(0.25)?;
    session.live().advance_segment_to(wait, wait.end_time())?;
    session.live().complete_segment(wait)?;
    assert_eq!(
        session.live().effective(&circle)?.transform.translation,
        Vec2::new(3.0, -2.0)
    );
    // Activation reads the edited effective value, not the declaration-time base.
    let segment = session.live().play_animation(&second)?;
    session
        .live()
        .advance_segment_to(segment, segment.end_time() - 1.0)?;
    assert_eq!(
        session.live().effective(&circle)?.transform.translation,
        Vec2::new(4.0, -2.0)
    );
    assert_eq!(
        session.live().authored(&circle)?.transform.translation.x,
        3.0
    );
    session
        .live()
        .advance_segment_to(segment, segment.end_time())?;
    session.live().complete_segment(segment)?;
    assert_eq!(
        session.live().effective(&circle)?.transform.translation,
        Vec2::new(5.0, -2.0)
    );
    assert_eq!(
        session.live().authored(&circle)?.transform.translation.x,
        5.0
    );
    assert!(session.live().segment_state(segment).is_complete());
    Ok(())
}
