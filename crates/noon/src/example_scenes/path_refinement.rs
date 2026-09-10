//! Curve queries and subdivision produce ordinary geometry through the shared scene.
use crate::{ExecutionSession, ManimGeometryOptions, Scene};
use noon_core::{Vec2, VectorPath};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut original = scene.geometry(ManimGeometryOptions::path(
            VectorPath::new()
                .move_to(Vec2::new(-3., 1.))
                .cubic_to(Vec2::new(-3., 3.), Vec2::new(0., 3.), Vec2::new(0., 1.))
                .line_to(Vec2::new(3., 1.)),
        )?)?;
        original.set_stroke_color(
            f64::from(noon_core::BLUE.red),
            f64::from(noon_core::BLUE.green),
            f64::from(noon_core::BLUE.blue),
            1.,
        )?;
        let mut refined = original.copy_handle()?;
        refined.insert_n_curves(3)?;
        refined.shift(0., -3.)?;
        refined.set_stroke_color(
            f64::from(noon_core::YELLOW.red),
            f64::from(noon_core::YELLOW.green),
            f64::from(noon_core::YELLOW.blue),
            1.,
        )?;
        let query = refined.path_query()?;
        assert_eq!(query.curve_count(), 5);
        assert_eq!(query.anchors_and_handles()[0].len(), 5);
        scene.add_many(&[(&original).into(), (&refined).into()])?;
        for (x, y) in query.start_anchors().into_iter().chain([query.end()?]) {
            let mut options = ManimGeometryOptions::dot(x, y, 0.06)?;
            options.set_color(
                f64::from(noon_core::RED.red),
                f64::from(noon_core::RED.green),
                f64::from(noon_core::RED.blue),
                1.,
            )?;
            let dot = scene.geometry(options)?;
            scene.add(&dot)?;
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
