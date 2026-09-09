//! Typed scenes for platform recovery and camera-density qualification.
use crate::{AnimationOptions, ExecutionSession, RateFunction, Scene};

pub fn circle() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let circle = scene.circle(0.75)?;
    scene.add(&circle).map_err(|error| error.to_string())?;
    scene.wait(4.0)?;
    scene.execution_session().map_err(|e| e.to_string())
}

pub fn four_animated() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.65)?;
    circle.shift(-2.0, 0.6)?;
    let mut rectangle = scene.rectangle(1.5, 0.9)?;
    rectangle.shift(2.0, 0.6)?;
    let mut line = scene.line((-1.2, 0.0), (1.2, 0.0))?;
    line.shift(-1.5, -1.4)?;
    let mut square = scene.square(0.8)?;
    square.shift(1.5, -1.4)?;
    scene
        .add_many(&[
            (&circle).into(),
            (&rectangle).into(),
            (&line).into(),
            (&square).into(),
        ])
        .map_err(|error| error.to_string())?;
    let mut target = circle.target_editor()?;
    target.shift(1.6, 0.0)?;
    let animation = scene.declare_transform_to(
        &circle,
        &target,
        AnimationOptions::new()
            .run_time(4.0)
            .rate_func(RateFunction::Linear),
    )?;
    let mut session = scene.execution_session().map_err(|e| e.to_string())?;
    scene
        .live(&mut session)
        .play_animation(&animation)
        .map_err(|e| e.to_string())?;
    Ok(session)
}

pub fn camera_density() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut rectangle = scene.rectangle(2.0, 1.0)?;
    rectangle.shift(1.25, -0.75)?;
    rectangle.rotate(std::f64::consts::PI / 6.0)?;
    rectangle.set_fill(1.0, 1.0, 1.0, 1.0)?;
    rectangle.disable_stroke()?;
    scene.add(&rectangle).map_err(|error| error.to_string())?;
    scene.wait(4.0)?;
    scene.execution_session().map_err(|e| e.to_string())
}
