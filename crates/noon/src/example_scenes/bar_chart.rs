//! Shared labeled BarChart scene for native and direct-WASM qualification.

use crate::plot_presentation::NumberLabelOptions;
use crate::{
    BarLabelOptions, ExecutionSession, LatexBackend, ManimBarChart, ManimBarChartOptions, Scene,
};

pub fn scene(backend: &mut impl LatexBackend) -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut options =
        ManimBarChartOptions::new(vec![-2.0, 0.0, 3.0, -1.0, 2.0], [-4.0, 4.0, 1.0], 8.0, 5.0);
    options.bar_names = Some(["A", "B", "C", "D", "E"].map(String::from).to_vec());
    let chart = ManimBarChart::create_with_axis_labels(
        std::rc::Rc::clone(scene.integration_store()),
        &options,
        &NumberLabelOptions {
            font: "DejaVu Sans Mono".into(),
            font_size: 36.0,
            buff: 0.25,
            direction: [-1.0, 0.0],
            ..Default::default()
        },
        backend,
    )?;
    let labels = chart.get_bar_labels(backend, &BarLabelOptions::default())?;
    scene.add_many(&[chart.family().into(), (&labels).into()])?;
    Ok(scene)
}

pub fn session(backend: &mut impl LatexBackend) -> Result<ExecutionSession, String> {
    scene(backend)
        .map_err(|error| error.to_string())?
        .execution_session()
        .map_err(|error| error.to_string())
}
