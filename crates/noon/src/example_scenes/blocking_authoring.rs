//! Ordinary synchronous helpers, control flow, and live post-play observations.
//!
//! The same function runs through a native event pump or a JSPI browser host.
//! Its paired Python example is `ordinary_blocking_authoring.py`.

use crate::{AnimationOptions, BlockingScene, Color, Mobject, RateFunction, ScriptError, Vec2};

fn slide(scene: &BlockingScene, square: &Mobject, expected_x: f64) -> Result<(), ScriptError> {
    scene.play_with(
        square.animate().shift(Vec2::new(1.0, 0.0)),
        AnimationOptions::new()
            .run_time(0.25)
            .rate_func(RateFunction::Linear),
    )?;
    let layout = scene.effective_layout(square)?;
    assert!((layout.center.0 - expected_x).abs() < 1e-5);
    Ok(())
}

/// Author one second of animation with no async function or scene syntax macro.
pub fn construct(scene: &BlockingScene) -> Result<(), ScriptError> {
    let square = scene.square(1.0)?;
    scene.set_fill(&square, Color::BLUE, 1.0)?;
    let line = scene.line(Vec2::ZERO, Vec2::new(-1.0, 0.0))?;
    scene.set_color(&line, Color::YELLOW)?;
    let turn = scene.add_updater(&line, |target, frame| {
        target.rotate_about_origin(frame.dt())
    })?;
    scene.add(&square)?;
    scene.add(&line)?;

    for index in 1..=2 {
        slide(scene, &square, f64::from(index))?;
    }
    assert_eq!(scene.time()?, 0.5);
    scene.wait(0.25)?;
    scene.remove_updater(&turn)?;
    assert!((scene.effective(&line)?.transform.rotation - 0.75).abs() < 1e-5);

    scene.set_fill(&square, Color::ORANGE, 1.0)?;
    scene.wait(0.25)?;
    assert_eq!(scene.time()?, 1.0);
    assert!((scene.effective(&line)?.transform.rotation - 0.75).abs() < 1e-5);
    Ok(())
}
