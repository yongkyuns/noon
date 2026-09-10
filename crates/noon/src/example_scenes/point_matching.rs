//! Shared retained point matching, paired with ordinary_point_matching.py.
use crate::{ExecutionSession, ManimGeometryOptions, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut source = scene.square(1.)?;
        source.set_stroke_color(88. / 255., 196. / 255., 221. / 255., 1.)?;
        let mut target = scene.geometry(ManimGeometryOptions::arc(1.4, -0.4, 4.7, 9, 0., 0.)?)?;
        target.rotate(0.2)?;
        source.match_points(&target)?;
        source.shift(-2.5, 0.)?;
        let mut line = scene.line((-1., 0.), (1., 0.))?;
        let mut ellipse = scene.geometry(ManimGeometryOptions::ellipse(2., 1.)?)?;
        ellipse.rotate(0.6)?;
        line.match_points(&ellipse)?;
        line.shift(2.5, 0.)?;
        scene.add_many(&[(&source).into(), (&line).into()])?;
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
