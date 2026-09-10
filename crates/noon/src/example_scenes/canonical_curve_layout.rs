//! Equivalent stretched Circle/Ellipse layout, paired with ordinary_canonical_curve_layout.py.
use crate::{ExecutionSession, ManimGeometryOptions, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut circle = scene.geometry(ManimGeometryOptions::circle(1.)?)?;
        circle.scale(2., 0.75)?;
        circle.set_color(1., 1., 1., 1.)?;
        let mut ellipse = scene.geometry(ManimGeometryOptions::ellipse(4., 1.5)?)?;
        ellipse.set_color(88. / 255., 196. / 255., 221. / 255., 1.)?;
        for (mut shape, x) in [(circle, -2.4), (ellipse, 2.4)] {
            shape.rotate(std::f64::consts::PI / 6.)?;
            shape.shift(x, 0.)?;
            let (right, center_y) = shape.critical_point(1., 0.)?;
            let mut marker = scene.square(0.12)?;
            marker.set_fill(
                f64::from(noon_core::YELLOW.red),
                f64::from(noon_core::YELLOW.green),
                f64::from(noon_core::YELLOW.blue),
                1.,
            )?;
            marker.set_stroke_width(0.)?;
            marker.shift(right, center_y)?;
            scene.add_many(&[(&shape).into(), (&marker).into()])?;
        }
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
