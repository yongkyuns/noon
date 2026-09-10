//! Curve-count extraction and direction changes through ordinary semantic edits.
use crate::{ExecutionSession, ManimGeometryOptions, Scene};
use noon_core::{Vec2, VectorPath};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut curve = scene.geometry(ManimGeometryOptions::path(
            VectorPath::new()
                .move_to(Vec2::new(-3., -1.))
                .cubic_to(Vec2::new(-3., 2.), Vec2::new(0., 2.), Vec2::new(0., -1.))
                .move_to(Vec2::new(1., -1.))
                .line_to(Vec2::new(3., 1.)),
        )?)?;
        curve.set_stroke_color(88. / 255., 196. / 255., 221. / 255., 1.)?;
        let mut selected = curve.copy_handle()?;
        selected.pointwise_become_partial(&curve, 0.2, 0.8)?;
        selected.shift(0., -2.)?;
        selected.set_stroke_color(
            f64::from(noon_core::YELLOW.red),
            f64::from(noon_core::YELLOW.green),
            f64::from(noon_core::YELLOW.blue),
            1.,
        )?;
        scene.add_many(&[(&curve).into(), (&selected).into()])?;
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        live.reverse_direction(&selected)?;
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
