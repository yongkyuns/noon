//! Explicitly sized semantic axes, a function and measured-data-shaped samples.
//! Samples are illustrative, not results from a navigation simulation.

use noon::{ManimAxesOptions, ManimGeometryOptions, Scene, Text, BLUE, YELLOW};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let axes = scene.axes(&ManimAxesOptions::new(
        [0.0, 10.0, 2.0],
        [-1.5, 1.5, 0.5],
        10.0,
        4.0,
    ))?;
    let frame = axes.authored_frame()?;
    let sampling = axes.plot_sampling(None)?;
    let mut curve = ManimGeometryOptions::axes_function_plot(
        frame, &sampling, |t| (t * 0.8).sin(), true,
    )?;
    curve.set_color(BLUE.red.into(), BLUE.green.into(), BLUE.blue.into(), 1.0)?;
    let curve = scene.geometry(curve)?;
    let samples = [[0.0, 0.1], [2.0, 0.95], [4.0, -0.1], [6.0, -0.9], [8.0, 0.2], [10.0, 1.0]];
    let mut data = ManimGeometryOptions::axes_sampled_plot(frame, &samples)?;
    data.set_color(YELLOW.red.into(), YELLOW.green.into(), YELLOW.blue.into(), 1.0)?;
    let data = scene.geometry(data)?;
    let mut title = scene.text(Text::new("Shared axes: function and sampled data").with_font_size(28.0))?;
    title.shift(0.0, 3.0)?;
    let mut label = scene.text(Text::new("Time (s)").with_font_size(22.0))?;
    label.shift(0.0, -2.7)?;
    scene.add_many(&[axes.family().into(), (&curve).into(), (&data).into(), (&title).into(), (&label).into()])?;
    noon_native::run(scene.execution_session()?)?;
    Ok(())
}
