//! Shared retained Matrix scene for native and direct WASM hosts.

use crate::{LatexBackend, Matrix, Scene};

pub fn build(backend: &mut impl LatexBackend) -> Result<(Scene, Matrix), String> {
    let mut scene = Scene::new();
    let matrix = Matrix::from_rows(&mut scene, backend, [["1", "2"], ["x", "y"]])
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[matrix.family().into()])
        .map_err(|error| error.to_string())?;
    Ok((scene, matrix))
}

pub fn session(backend: &mut impl LatexBackend) -> Result<crate::SceneExecution, String> {
    let (scene, _matrix) = build(backend)?;
    scene.into_execution().map_err(|error| error.to_string())
}
