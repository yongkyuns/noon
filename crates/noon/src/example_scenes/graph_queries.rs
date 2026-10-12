//! Explicit graph inputs, paired with `web/python/examples/graph_queries.py`.
//! Current axes queries leave the already sampled graph unchanged.
use crate::{
    ExecutionSession, ManimAxesOptions, ManimGeometryOptions, PlotSamplingOptions, Scene, BLUE,
    GREEN, ORANGE, RED, YELLOW,
};

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let axes = scene.axes(&ManimAxesOptions::new(
        [-2.0, 2.0, 1.0],
        [-1.0, 1.0, 1.0],
        4.0,
        2.0,
    ))?;
    let function = |x: f64| 0.5 * x * x;
    let mut graph = axes.plot(function, Some(&[-1.0, 1.0, 0.25]), false)?;
    graph.set_color(BLUE.red.into(), BLUE.green.into(), BLUE.blue.into(), 1.0)?;
    let [low, high] = graph.function_plot_range()?;
    let start = axes.input_to_graph_point(low, function)?;
    let end = axes.input_to_graph_point(high, function)?;
    axes.family().shift(0.0, 0.75)?;
    let current = axes.input_to_graph_point(0.0, function)?;

    let world_function = |x: f64| -2.0 + 0.25 * x;
    let mut world_graph = scene.function_plot(
        &PlotSamplingOptions::parametric(&[-1.0, 1.0, 0.5])?,
        world_function,
        false,
    )?;
    world_graph.set_color(GREEN.red.into(), GREEN.green.into(), GREEN.blue.into(), 1.0)?;
    let [world_low, world_high] = world_graph.function_plot_range()?;
    scene.add_many(&[axes.family().into(), (&graph).into(), (&world_graph).into()])?;
    for (point, color) in [
        (start, ORANGE),
        (end, YELLOW),
        (current, RED),
        ([world_low, world_function(world_low)], ORANGE),
        ([world_high, world_function(world_high)], YELLOW),
    ] {
        let mut marker = scene.geometry(ManimGeometryOptions::dot(point[0], point[1], 0.08)?)?;
        marker.set_color(color.red.into(), color.green.into(), color.blue.into(), 1.0)?;
        scene.add(&marker)?;
    }
    Ok(scene)
}

pub fn session() -> Result<ExecutionSession, String> {
    scene()
        .map_err(|error| error.to_string())?
        .execution_session()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_queries_add_no_runtime_callbacks_or_resource_churn() {
        let scene = scene().unwrap();
        let revision = scene.revision();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats();
        let mut session = scene.execution_session().unwrap();
        assert_eq!(session.frame().objects.len(), 15);
        assert!(!session.has_required_callbacks());
        session.take_renderer_publication();
        session.seek(1.0).unwrap();
        assert_eq!(scene.revision(), revision);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .stats(),
            resources
        );
    }
}
