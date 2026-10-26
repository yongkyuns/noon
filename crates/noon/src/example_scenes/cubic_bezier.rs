//! The same four-control cubic used by the ordinary Python constructor fixture.
use crate::{ManimGeometryOptions, Scene, Vec2, VectorPath};

pub fn session() -> Result<crate::SceneExecution, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let path = VectorPath::new().move_to(Vec2::new(-3., -1.)).cubic_to(
            Vec2::new(-1., 2.),
            Vec2::new(1., -2.),
            Vec2::new(3., 1.),
        );
        let mut curve = scene.geometry(ManimGeometryOptions::path(path)?)?;
        curve.set_stroke_color(88. / 255., 196. / 255., 221. / 255., 1.)?;
        // Rust stores normalized width; the Python adapter converts Manim points.
        curve.set_stroke_width(0.06)?;
        assert_eq!(curve.path_query()?.point_count(), 4);
        scene.add(&curve)?;
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(scene.into_execution_with_session(session)?)
    };
    build().map_err(|error| error.to_string())
}
