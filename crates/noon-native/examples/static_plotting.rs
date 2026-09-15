//! Static function and sampled-data paths through shared Rust authoring.
//! This example is not the still-pending Axes/NumberLine public facade.

use noon::{ManimGeometryOptions, PlotSamplingOptions, Scene};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let x_axis = scene.line((-4.5, 0.0), (4.5, 0.0))?;
    let y_axis = scene.line((0.0, -2.0), (0.0, 2.0))?;
    scene.add(&x_axis)?;
    scene.add(&y_axis)?;

    let sampling = PlotSamplingOptions::parametric(&[-4.0, 4.0, 0.05])?;
    let mut function = ManimGeometryOptions::function_plot(&sampling, f64::sin, true)?;
    function.set_color(0.2, 0.6, 1.0, 1.0)?;
    let curve = scene.geometry(function)?;
    scene.add(&curve)?;

    // Authored illustrative samples, not claimed to be navigation simulation data.
    let mut data = ManimGeometryOptions::sampled_plot(&[
        [-4.0, 0.6],
        [-3.0, -0.2],
        [-2.0, -0.8],
        [-1.0, -0.6],
        [0.0, 0.2],
        [1.0, 0.9],
        [2.0, 0.7],
        [3.0, 0.1],
        [4.0, -0.6],
    ])?;
    data.set_color(1.0, 0.8, 0.2, 1.0)?;
    let samples = scene.geometry(data)?;
    scene.add(&samples)?;

    noon_native::run(scene.execution_session()?)?;
    Ok(())
}
