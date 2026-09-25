//! Shared persistent DecimalNumber example for native and direct WASM hosts.

use crate::{DecimalFormat, DecimalNumber, ExecutionSession, LatexBackend, Scene, ValueTracker};
use std::rc::Rc;

pub const INITIAL_VALUE: f64 = -0.004;
pub const DISPLAY_VALUE: f64 = 12_345.6;
pub const TRACK_DURATION: f64 = 2.0;

pub fn build(backend: &mut impl LatexBackend) -> Result<(Scene, DecimalNumber), String> {
    let mut scene = Scene::new();
    let mut number = DecimalNumber::new(
        Rc::clone(scene.integration_store()),
        backend,
        INITIAL_VALUE,
        DecimalFormat {
            include_sign: true,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    number
        .set_value(backend, DISPLAY_VALUE)
        .map_err(|error| error.to_string())?;
    scene
        .add(number.mobject())
        .map_err(|error| error.to_string())?;
    Ok((scene, number))
}

pub fn session(backend: &mut impl LatexBackend) -> Result<ExecutionSession, String> {
    let (scene, _number, _tracker) = build_variable(backend)?;
    scene.execution_session().map_err(|error| error.to_string())
}

/// Shared native/direct-WASM proof for tracker-driven numeric content.
pub fn build_variable(
    backend: &mut impl LatexBackend,
) -> Result<(Scene, DecimalNumber, ValueTracker), String> {
    let mut scene = Scene::new();
    let tracker = scene
        .value_tracker(INITIAL_VALUE)
        .map_err(|error| error.to_string())?;
    let mut number = DecimalNumber::new(
        Rc::clone(scene.integration_store()),
        backend,
        INITIAL_VALUE,
        DecimalFormat {
            include_sign: true,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    number
        .bind_to_tracker(backend, &tracker)
        .map_err(|error| error.to_string())?;
    scene
        .add(number.mobject())
        .map_err(|error| error.to_string())?;
    scene
        .play_value(&tracker, DISPLAY_VALUE)
        .run_time(TRACK_DURATION)?;
    Ok((scene, number, tracker))
}
