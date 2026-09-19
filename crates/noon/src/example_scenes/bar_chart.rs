//! Shared static BarChart scene for native and direct-WASM qualification.

use crate::{ExecutionSession, ManimBarChartOptions, Scene};

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let chart = scene.bar_chart(&ManimBarChartOptions::new(
        vec![-2.0, 0.0, 3.0, -1.0, 2.0],
        [-4.0, 4.0, 1.0],
        8.0,
        5.0,
    ))?;
    scene.add_many(&[chart.family().into()])?;
    Ok(scene)
}

pub fn session() -> Result<ExecutionSession, String> {
    scene()
        .map_err(|error| error.to_string())?
        .execution_session()
        .map_err(|error| error.to_string())
}
