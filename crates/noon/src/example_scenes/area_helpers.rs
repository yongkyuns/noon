//! One shared builder for native, direct-WASM and Python area examples.
use crate::{
    ExecutionSession, ManimAxesOptions, RiemannRectangleOptions, RiemannSample, Scene, Text, BLUE,
    GREEN,
};

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let axes = scene.axes(&ManimAxesOptions::new(
        [-3.0, 3.0, 1.0],
        [-2.0, 4.0, 1.0],
        8.0,
        5.0,
    ))?;
    let function = |x: f64| 0.25 * x * x - 0.5;
    let mut graph = axes.plot(function, Some(&[-3.0, 3.0, 0.25]), false)?;
    graph.set_color(BLUE.red.into(), BLUE.green.into(), BLUE.blue.into(), 1.0)?;
    let mut area = axes.get_area(&graph, Some([-2.0, 0.0]), None)?;
    area.set_color(GREEN.red.into(), GREEN.green.into(), GREEN.blue.into(), 1.0)?;
    let plan = axes.riemann_plan(
        &graph,
        RiemannRectangleOptions {
            x_range: Some([0.0, 2.0]),
            dx: 0.25,
            sample: RiemannSample::Center,
            ..Default::default()
        },
    )?;
    let values = plan.samples().map(function).collect::<Vec<_>>();
    let rectangles = plan.publish(Some(&values), None)?;
    let mut title = scene.text(Text::new("Area and Riemann rectangles").with_font_size(28.0))?;
    title.shift(0.0, 3.0)?;
    let mut caption = scene.text(
        Text::new("Area on the left | midpoint samples on the right | negative area inverts color")
            .with_font_size(16.0),
    )?;
    caption.shift(0.0, -3.0)?;
    scene.add_many(&[
        axes.family().into(),
        (&area).into(),
        (&rectangles).into(),
        (&graph).into(),
        (&title).into(),
        (&caption).into(),
    ])?;
    Ok(scene)
}

pub fn session() -> Result<ExecutionSession, String> {
    let scene = scene().map_err(|error| error.to_string())?;
    scene.execution_session().map_err(|error| error.to_string())
}
