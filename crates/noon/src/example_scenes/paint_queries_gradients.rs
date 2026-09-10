//! Shared family gradient and paint observations, paired with ordinary_paint_queries_gradients.py.
use crate::{Color, ExecutionSession, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut boxes = Vec::new();
        for index in 0..5 {
            let mut object = scene.square(1.0)?;
            object.shift((index as f64 - 2.0) * 1.5, 0.0)?;
            boxes.push(object);
        }
        let members: Vec<_> = boxes.iter().map(Into::into).collect();
        let nested = scene.family(&members)?;
        let family = scene.family(&[(&boxes[0]).into(), (&nested).into()])?;
        family.set_fill(None, Some(0.7))?;
        family.set_stroke(None, Some(0.02), None)?;
        scene.add_many(&[(&family).into()])?;
        let mut session = scene.execution_session()?;
        {
            let mut live = scene.live(&mut session);
            live.set_family_color_by_gradient(
                &family,
                &[
                    Color::rgb(1.0, 0.0, 0.0),
                    Color::rgb(0.0, 1.0, 0.0),
                    Color::rgb(0.0, 0.0, 1.0),
                ],
            )?;
            let color = live.effective_manim_color(&boxes[0])?;
            live.set_color(
                &boxes[0],
                color.red.into(),
                color.green.into(),
                color.blue.into(),
                color.alpha.into(),
            )?;
            assert!((live.effective(&boxes[0])?.fill_opacity() - 0.7).abs() < 1e-6);
            assert!((live.effective_stroke_width(&boxes[0])? - 0.02).abs() < 1e-6);
            let segment = live.wait_segment(0.2)?;
            live.advance_segment_to(segment, segment.end_time())?;
            live.complete_segment(segment)?;
        }
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
