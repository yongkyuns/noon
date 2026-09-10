//! Unequal paths align through one shared transaction and remain ordinary shapes.
use crate::{ExecutionSession, ManimGeometryOptions, Scene};
use noon_core::{Vec2, VectorPath};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut line = scene.line((-3., -1.), (-1., -1.))?;
        let mut wave = scene.geometry(ManimGeometryOptions::path(
            VectorPath::new().move_to(Vec2::new(1., -1.)).cubic_to(
                Vec2::new(1., 2.),
                Vec2::new(3., 2.),
                Vec2::new(3., -1.),
            ),
        )?)?;
        wave.insert_n_curves(4)?;
        line.align_points(&wave)?;
        line.set_color(
            f64::from(noon_core::BLUE.red),
            f64::from(noon_core::BLUE.green),
            f64::from(noon_core::BLUE.blue),
            1.,
        )?;
        wave.set_color(
            f64::from(noon_core::YELLOW.red),
            f64::from(noon_core::YELLOW.green),
            f64::from(noon_core::YELLOW.blue),
            1.,
        )?;
        scene.add(&line)?;
        scene.add(&wave)?;
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(session)
    };
    build().map_err(|e| e.to_string())
}
