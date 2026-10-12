//! Paired implicit-contour gallery: two scalar fields become retained paths.
use crate::{
    ExecutionSession, ImplicitPlotOptions, ManimAxesOptions, ManimGeometryOptions, Scene, BLUE,
    YELLOW,
};

#[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
use crate::Text;

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let axes = scene.axes(&ManimAxesOptions::new(
        [-3.0, 3.0, 1.0],
        [-2.0, 2.0, 1.0],
        9.0,
        4.8,
    ))?;
    axes.family().shift(0.0, -0.15)?;
    let frame = axes.authored_frame()?;
    let mut options = ImplicitPlotOptions::default();
    options.bounds.min = noon_geometry::IsolinePoint::new(-3.0, -2.0);
    options.bounds.max = noon_geometry::IsolinePoint::new(3.0, 2.0);
    options.contour.min_depth = 4;
    options.contour.max_quads = 600;
    let mut circle = scene.geometry(ManimGeometryOptions::axes_implicit_plot(
        frame,
        &options,
        |x, y| x * x + y * y - 2.25,
    )?)?;
    circle.set_color(BLUE.red.into(), BLUE.green.into(), BLUE.blue.into(), 1.0)?;
    let mut hyperbola = scene.geometry(ManimGeometryOptions::axes_implicit_plot(
        frame,
        &options,
        |x, y| x * y - 0.65,
    )?)?;
    hyperbola.set_color(
        YELLOW.red.into(),
        YELLOW.green.into(),
        YELLOW.blue.into(),
        1.0,
    )?;
    #[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
    {
        let mut title = scene
            .text(Text::new("Implicit curves: sampled once, retained").with_font_size(28.0))?;
        title.shift(0.0, 3.1)?;
        let mut caption =
            scene.text(Text::new("x² + y² = 2.25     |     xy = 0.65").with_font_size(24.0))?;
        caption.shift(0.0, -3.1)?;
        scene.add_many(&[
            axes.family().into(),
            (&circle).into(),
            (&hyperbola).into(),
            (&title).into(),
            (&caption).into(),
        ])?;
    }
    #[cfg(not(all(feature = "native-text", feature = "bundled-fonts")))]
    scene.add_many(&[axes.family().into(), (&circle).into(), (&hyperbola).into()])?;
    Ok(scene)
}

pub fn session() -> Result<ExecutionSession, String> {
    let scene = scene().map_err(|error| error.to_string())?;
    scene.execution_session().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_has_no_frame_callbacks_or_resource_regeneration() {
        let scene = scene().unwrap();
        let revision = scene.revision();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats();
        let mut execution = scene.execution_session().unwrap();
        assert_eq!(
            execution.frame().objects.len(),
            if cfg!(all(feature = "native-text", feature = "bundled-fonts")) {
                16
            } else {
                14
            }
        );
        assert!(!execution.has_required_callbacks());
        execution.take_renderer_publication();
        execution.seek(0.75).unwrap();
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
