//! A family of open and closed contours shares one smoothing operation.
use crate::{ExecutionSession, ManimGeometryOptions, Scene};
use noon_core::{Vec2, VectorPath};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut open = scene.geometry(ManimGeometryOptions::path(VectorPath::new())?)?;
        open.set_points_as_corners(&[Vec2::new(-3., 1.), Vec2::new(-2., 2.), Vec2::new(-1., 1.)])?;
        let mut closed = scene.square(1.5)?;
        closed.shift(2., 1.5)?;
        let family = scene.family(&[(&open).into(), (&closed).into()])?;
        family.set_color(
            f64::from(noon_core::BLUE.red),
            f64::from(noon_core::BLUE.green),
            f64::from(noon_core::BLUE.blue),
            1.,
        )?;
        let smoothed = family.copy_family()?;
        let smoothed = smoothed.root();
        smoothed.make_smooth()?;
        smoothed.shift(0., -3.)?;
        smoothed.set_color(
            f64::from(noon_core::YELLOW.red),
            f64::from(noon_core::YELLOW.green),
            f64::from(noon_core::YELLOW.blue),
            1.,
        )?;
        scene.add_many(&[(&family).into(), smoothed.into()])?;
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
