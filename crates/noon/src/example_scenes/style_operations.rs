//! Paired common style updates and matching on the ordinary retained path.
use crate::{Color, ExecutionSession, Scene, StyleUpdate};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut a = scene.square(1.0)?;
        let b = scene.circle(0.5)?;
        let mut c = scene.square(1.0)?;
        a.shift(-2.0, 0.0)?;
        c.shift(2.0, 0.0)?;
        let nested = scene.family(&[(&a).into(), (&b).into()])?;
        let family = scene.family(&[(&a).into(), (&nested).into()])?;
        let palette = family.copy_family()?.root().clone();
        palette.set_style(StyleUpdate {
            fill_color: Some(Color::rgba(0.0, 0.0, 1.0, 1.0)),
            fill_opacity: Some(0.7),
            stroke_color: Some(Color::rgba(1.0, 0.0, 0.0, 1.0)),
            stroke_width: Some(0.06),
            ..Default::default()
        })?;
        family.match_style(&palette)?;
        scene.add_many(&[(&family).into(), (&c).into()])?;
        let mut session = scene.execution_session()?;
        {
            let mut live = scene.live(&mut session);
            live.match_style(&c, &a)?;
            live.set_family_style(
                &family,
                StyleUpdate {
                    fill_color: Some(Color::rgba(1.0, 128.0 / 255.0, 0.0, 1.0)),
                    ..Default::default()
                },
            )?;
            live.set_family_fill(&family, None, None)?;
            live.set_family_stroke(&family, None, None, None)?;
            let segment = live.wait_segment(0.2)?;
            live.advance_segment_to(segment, segment.end_time())?;
            live.complete_segment(segment)?;
        }
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
