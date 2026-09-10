//! Shared persistent corner editing, paired with ordinary_path_editing.py.
use crate::{ExecutionSession, ManimGeometryOptions, Scene};
use noon_core::{Vec2, VectorPath};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut path = scene.geometry(ManimGeometryOptions::path(VectorPath::new())?)?;
        path.set_stroke_color(88. / 255., 196. / 255., 221. / 255., 1.)?;
        path.set_points_as_corners(&[
            Vec2::new(-3., -1.),
            Vec2::new(-2., 1.),
            Vec2::new(-1., -1.),
        ])?;
        let mut original = path.copy_handle()?;
        original.shift(0., 2.5)?;
        scene.add_many(&[(&path).into(), (&original).into()])?;
        let mut polygon = scene.square(1.)?;
        polygon.shift(7., 3.)?;
        polygon.rotate(0.6)?;
        polygon.set_fill(1., 1., 0., 0.3)?;
        polygon.set_points_as_corners(&[
            Vec2::new(1., -1.),
            Vec2::new(3., -1.),
            Vec2::new(2., 1.),
            Vec2::new(1., -1.),
        ])?;
        scene.add(&polygon)?;
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        live.set_points_as_corners(
            &path,
            &[Vec2::new(-3., -1.), Vec2::new(-2., 0.), Vec2::new(-1., -1.)],
        )?;
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
