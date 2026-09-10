//! Shared center/edge/point scaling, paired with ordinary_scale_pivots.py.
use crate::{ExecutionSession, LayoutAnchor, ManimRotationPivot as Pivot, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut line = scene.line((-3., 1.), (-1., 1.5))?;
        line.set_stroke_color(68. / 255., 136. / 255., 1., 1.)?;
        line.manim_scale(1.5, 1.5)?;
        let mut other = scene.line((1., 1.), (3., 1.5))?;
        other.set_stroke_color(68. / 255., 1., 136. / 255., 1.)?;
        other.manim_scale_about_edge(1.5, 1.5, 1., 0.)?;
        let mut a = scene.square(0.6)?;
        a.set_fill(1., 68. / 255., 102. / 255., 1.)?;
        a.set_stroke_width(0.)?;
        a.shift(-1., -1.)?;
        let mut b = scene.square(0.6)?;
        b.set_fill(1., 204. / 255., 68. / 255., 1.)?;
        b.set_stroke_width(0.)?;
        b.shift(1., -1.)?;
        let nested = scene.family(&[(&a).into(), (&b).into()])?;
        let family = scene.family(&[(&a).into(), (&nested).into()])?;
        LayoutAnchor::from(&family).scale(1.25, 1.25, Pivot::Point(0., 0.))?;
        scene.add_many(&[(&line).into(), (&other).into(), (&family).into()])?;
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        live.manim_scale(&line, 0.8, 0.8)?;
        live.manim_scale_about_point(&other, 0.8, 0.8, 2., 1.)?;
        live.scale_layout(&LayoutAnchor::from(&family), 1.2, 1.2, Pivot::Edge(-1., 0.))?;
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
