//! Shared Variable example for native, direct WASM, and Python authoring.

use crate::{DecimalFormat, ExecutionSession, LatexBackend, MobjectTarget, Scene};

pub const VALUE: f64 = 12_345.6;

pub fn session(backend: &mut impl LatexBackend) -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let variable = scene
        .variable(backend, "x", VALUE, DecimalFormat::default(), 48.0)
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[MobjectTarget::Family(variable.family())])
        .map_err(|error| error.to_string())?;
    scene.execution_session().map_err(|error| error.to_string())
}
