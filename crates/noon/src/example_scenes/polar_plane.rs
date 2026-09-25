//! Retained polar coordinates, paired with `web/python/examples/polar_plane.py`.
use crate::{
    ExecutionSession, ManimGeometryOptions, ManimPolarPlaneOptions, PolarAzimuthDirection, Scene,
    SemanticPaint, BLUE, YELLOW,
};

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut options = ManimPolarPlaneOptions {
        radius_max: 3.0,
        size: Some(6.0),
        radius_step: 1.0,
        azimuth_step: Some(12.0),
        azimuth_offset: std::f64::consts::FRAC_PI_6,
        azimuth_direction: PolarAzimuthDirection::Clockwise,
        faded_line_ratio: 2,
        ..Default::default()
    };
    options.background_line_style.stroke = Some(SemanticPaint::Solid(BLUE));
    options.background_line_style.stroke_width = 0.01;
    let plane = scene.polar_plane(&options)?;
    let frame = plane.authored_polar_frame()?;
    let point = frame.polar_to_point(2.0, std::f64::consts::FRAC_PI_3)?;
    let roundtrip = frame.point_to_polar(point)?;
    assert!((roundtrip[0] - 2.0).abs() < 1.0e-6);
    let mut marker = scene.geometry(ManimGeometryOptions::dot(point[0], point[1], 0.08)?)?;
    marker.set_color(
        YELLOW.red.into(),
        YELLOW.green.into(),
        YELLOW.blue.into(),
        1.0,
    )?;
    let content = scene.family(&[plane.family().into(), (&marker).into()])?;
    scene.add_many(&[(&content).into()])?;
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
    fn polar_demo_is_static_and_keeps_retained_resources_idle() {
        let scene = scene().unwrap();
        let revision = scene.revision();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats();
        let mut session = scene.execution_session().unwrap();
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
