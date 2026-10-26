//! Shared multi-subpath construction with line, quadratic and cubic segments.
use crate::{ManimGeometryOptions, Scene};
use noon_core::VectorPath;

pub fn session() -> Result<crate::SceneExecution, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut curves = scene.geometry(ManimGeometryOptions::path(VectorPath::new())?)?;
        curves.set_stroke_color(88. / 255., 196. / 255., 221. / 255., 1.)?;
        curves.start_new_path_f64((-3., -1.))?;
        curves.add_line_to_f64((-2., 0.))?;
        curves.add_quadratic_bezier_curve_to_f64((-1., 2.), (0., 0.))?;
        curves.start_new_path_f64((1., -1.))?;
        curves.add_cubic_bezier_curve_to_f64((1., 2.), (3., 2.), (3., -1.))?;
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
            crate::integration::BorrowedPathEdit::AppendCornersF64 {
                object: &polygon,
                points: &[(0., -2.)],
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
        Ok(scene.into_execution_with_session(session)?)
    };
    build().map_err(|error| error.to_string())
}
