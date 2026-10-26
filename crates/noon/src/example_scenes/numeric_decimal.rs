//! Shared persistent DecimalNumber example for native and direct WASM hosts.

use crate::{DecimalFormat, DecimalNumber, LatexBackend, Scene};
use std::rc::Rc;

pub const INITIAL_VALUE: f64 = -0.004;
pub const DISPLAY_VALUE: f64 = 12_345.6;

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

pub fn session(backend: &mut impl LatexBackend) -> Result<crate::SceneExecution, String> {
    let (scene, _number) = build(backend)?;
    scene.into_execution().map_err(|error| error.to_string())
}
