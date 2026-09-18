//! One retained coordinate grid, curve and marker, paired with number_plane.py.
use crate::{
    ExecutionSession, ManimNumberPlaneOptions, Scene, SemanticPaint, Text, BLUE, GREEN, YELLOW,
};

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut options = ManimNumberPlaneOptions {
        x_range: [-3.0, 3.0, 1.0],
        y_range: [-2.0, 2.0, 1.0],
        x_length: Some(8.0),
        y_length: Some(4.5),
        faded_line_ratio: 2,
        ..Default::default()
    };
    options.background_line_style.stroke = Some(SemanticPaint::Solid(GREEN));
    options.background_line_style.stroke_width = 0.01;
    let plane = scene.number_plane(&options)?;
    let frame = plane.authored_frame()?;
    let sampling = crate::PlotSamplingOptions::axes(frame.x().range(), Some(&[-3.0, 3.0, 0.05]))?;
    let mut curve = scene.geometry(crate::ManimGeometryOptions::axes_function_plot(
        frame,
        &sampling,
        |x| 0.35 * x * x - 1.0,
        true,
    )?)?;
    curve.set_color(BLUE.red.into(), BLUE.green.into(), BLUE.blue.into(), 1.0)?;
    let point = frame.coords_to_point(1.0, -0.65)?;
    let roundtrip = frame.point_to_coords(point)?;
    assert!((roundtrip[0] - 1.0).abs() < 1e-6 && (roundtrip[1] + 0.65).abs() < 1e-6);
    let mut marker = scene.geometry(crate::ManimGeometryOptions::dot(point[0], point[1], 0.08)?)?;
    marker.set_color(
        YELLOW.red.into(),
        YELLOW.green.into(),
        YELLOW.blue.into(),
        1.0,
    )?;
    let content = scene.family(&[plane.family().into(), (&curve).into(), (&marker).into()])?;
    content.scale(0.85, 0.85)?;
    content.shift(0.0, -0.1)?;
    let mut title =
        scene.text(Text::new("NumberPlane: a shared coordinate grid").with_font_size(28.0))?;
    title.shift(0.0, 3.0)?;
    let mut caption = scene.text(
        Text::new("Major lines + subdivisions | y = 0.35 x² - 1 | point (1, -0.65)")
            .with_font_size(20.0),
    )?;
    caption.shift(0.0, -3.0)?;
    scene.add_many(&[(&content).into(), (&title).into(), (&caption).into()])?;
    Ok(scene)
}

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let scene = scene()?;
        Ok(scene.execution_session()?)
    };
    build().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_demo_is_static_and_seek_does_not_rebuild_resources() {
        let scene = scene().unwrap();
        let revision = scene.revision();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats();
        let mut session = scene.execution_session().unwrap();
        assert!(!session.has_required_callbacks());
        assert_eq!(session.frame().objects.len(), 24);
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
