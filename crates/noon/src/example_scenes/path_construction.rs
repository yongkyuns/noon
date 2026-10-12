//! Shared multi-subpath construction with line, quadratic and cubic segments.
use crate::{ExecutionSession, ManimGeometryOptions, Scene};
use noon_core::{Vec2, VectorPath};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut curves = scene.geometry(ManimGeometryOptions::path(VectorPath::new())?)?;
        curves.set_stroke_color(88. / 255., 196. / 255., 221. / 255., 1.)?;
        curves.start_new_path(Vec2::new(-3., -1.))?;
        curves.add_line_to(Vec2::new(-2., 0.))?;
        curves.add_quadratic_bezier_curve_to(Vec2::new(-1., 2.), Vec2::new(0., 0.))?;
        curves.start_new_path(Vec2::new(1., -1.))?;
        curves.add_cubic_bezier_curve_to(
            Vec2::new(1., 2.),
            Vec2::new(3., 2.),
            Vec2::new(3., -1.),
        )?;
        curves.close_path()?;
        let mut polygon = scene.square(1.)?;
        polygon.set_fill(
            f64::from(noon_core::YELLOW.red),
            f64::from(noon_core::YELLOW.green),
            f64::from(noon_core::YELLOW.blue),
            0.3,
        )?;
        polygon.shift(-2., -2.)?;
        scene.add_many(&[(&curves).into(), (&polygon).into()])?;
        let mut session = scene.execution_session()?;
        crate::integration::publish_borrowed_path_edit(
            scene.integration_store(),
            scene.root(),
            &mut session,
            crate::integration::BorrowedPathEdit::Line {
                object: &polygon,
                point: Vec2::new(0., -2.),
            },
        )?;
        crate::integration::publish_borrowed_path_edit(
            scene.integration_store(),
            scene.root(),
            &mut session,
            crate::integration::BorrowedPathEdit::Close { object: &polygon },
        )?;
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
