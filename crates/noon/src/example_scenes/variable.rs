//! Shared Variable example for native, direct WASM, and Python authoring.

use crate::{DecimalFormat, LatexBackend, MobjectTarget, Scene};

pub const VALUE: f64 = 12_345.6;

pub fn session(backend: &mut impl LatexBackend) -> Result<crate::SceneExecution, String> {
    let mut scene = Scene::new();
    let variable = scene
        .variable(backend, "x", VALUE, DecimalFormat::default(), 48.0)
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[MobjectTarget::Family(variable.family())])
        .map_err(|error| error.to_string())?;
    scene.into_execution().map_err(|error| error.to_string())
}
