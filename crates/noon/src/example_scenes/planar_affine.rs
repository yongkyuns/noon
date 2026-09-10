//! Paired with ordinary_planar_affine.py on native and direct Rust/WASM.
use crate::{ExecutionSession, LayoutAnchor, ManimRotationPivot as Pivot, Scene, SemanticVec3};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut rectangle = scene.rectangle(1.5, 0.7)?;
        rectangle.set_fill(1., 68. / 255., 102. / 255., 1.)?;
        rectangle.set_stroke_width(0.)?;
        rectangle.rotate_with_pivot(0.3, Pivot::Center)?;
        rectangle.shift(-2., 1.)?;
        let mut line = scene.line((-3., -1.), (-1., 0.))?;
        line.set_stroke_color(68. / 255., 136. / 255., 1., 1.)?;
        let mut reflected_rectangle = rectangle.copy_handle()?;
        reflected_rectangle.set_fill(1., 204. / 255., 68. / 255., 1.)?;
        let mut reflected_line = line.copy_handle()?;
        reflected_line.set_stroke_color(68. / 255., 1., 136. / 255., 1.)?;
        let nested = scene.family(&[(&reflected_rectangle).into(), (&reflected_line).into()])?;
        let reflected = scene.family(&[(&reflected_rectangle).into(), (&nested).into()])?;
        reflected.flip(SemanticVec3::new(0., 1., 0.), Pivot::Point(0., 0.))?;
        scene.add_many(&[(&rectangle).into(), (&line).into(), (&reflected).into()])?;
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        let anchor = LayoutAnchor::from(&reflected);
        live.rotate_layout(&anchor, std::f64::consts::PI / 6., Pivot::Point(2., 0.))?;
        live.flip_layout(&anchor, SemanticVec3::new(1., 1., 0.), Pivot::Center)?;
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(session)
    };
    build().map_err(|e| e.to_string())
}
