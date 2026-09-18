//! Paired plotting example shared unchanged by native and direct Rust/WASM.
//! Samples are illustrative, not navigation simulation results.

use crate::{
    ExecutionSession, ManimAxesOptions, ManimNumberLineOptions, Scene, SemanticPaint, Text, BLUE,
    GREEN, YELLOW,
};

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let axes = scene.axes(&ManimAxesOptions::new(
        [0.0, 10.0, 2.0],
        [-1.5, 1.5, 0.5],
        10.0,
        4.0,
    ))?;
    let mut curve = axes.plot(|t| (t * 0.8).sin(), Some(&[0.0, 10.0, 0.05]), true)?;
    curve.set_color(BLUE.red.into(), BLUE.green.into(), BLUE.blue.into(), 1.0)?;
    let samples = [
        [0.0, 0.1],
        [2.0, 0.95],
        [4.0, -0.1],
        [6.0, -0.9],
        [8.0, 0.2],
        [10.0, 1.0],
    ];
    let mut data = axes.plot_samples(&samples)?;
    data.set_color(
        YELLOW.red.into(),
        YELLOW.green.into(),
        YELLOW.blue.into(),
        1.0,
    )?;
    let mut title =
        scene.text(Text::new("Shared axes: function and sampled data").with_font_size(28.0))?;
    title.shift(0.0, 3.0)?;
    let mut label = scene.text(Text::new("Time (s)").with_font_size(22.0))?;
    label.shift(0.0, -2.7)?;
    let mut interval_options = ManimNumberLineOptions::unit_interval();
    interval_options.unit_size = 4.0;
    interval_options.style.stroke = Some(SemanticPaint::Solid(GREEN));
    let interval = scene.number_line(&interval_options)?;
    interval.family().shift(0.0, -3.45)?;
    let mut interval_label = scene.text(Text::new("Unit interval").with_font_size(16.0))?;
    interval_label.shift(-3.5, -3.45)?;
    scene.add_many(&[
        axes.family().into(),
        (&curve).into(),
        (&data).into(),
        (&title).into(),
        (&label).into(),
        interval.family().into(),
        (&interval_label).into(),
    ])?;
    Ok(scene)
}

pub fn session() -> Result<ExecutionSession, Box<dyn std::error::Error>> {
    Ok(scene()?.execution_session()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_plot_is_static_and_reuses_its_retained_resources() {
        let scene = scene().unwrap();
        let revision = scene.revision();
        let mut session = scene.execution_session().unwrap();
        let object_count = session.frame().objects.len();
        assert_eq!(object_count, 30);
        assert!(!session.has_required_callbacks());
        session.take_renderer_publication();
        session.seek(0.75).unwrap();
        assert_eq!(session.frame().objects.len(), object_count);
        assert_eq!(scene.revision(), revision);
    }
}
