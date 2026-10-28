//! Paired Circle.surround sizing on authored and live native/direct-WASM paths.
use crate::{LayoutAnchor, Scene};

pub fn session() -> Result<crate::SceneExecution, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut left_target = scene.rectangle(2.0, 1.0)?;
        let mut right_target = scene.rectangle(2.0, 1.0)?;
        let mut left_circle = scene.circle(0.4)?;
        let mut right_circle = scene.circle(0.4)?;
        for (object, color, x) in [
            (&mut left_target, (68.0 / 255.0, 136.0 / 255.0, 1.0), -3.0),
            (&mut right_target, (1.0, 68.0 / 255.0, 102.0 / 255.0), 3.0),
        ] {
            object.set_fill(color.0, color.1, color.2, 0.4)?;
            object.set_stroke_width(0.0)?;
            object.shift(x, 0.0)?;
        }
        for (object, color) in [
            (
                &mut left_circle,
                (88.0 / 255.0, 196.0 / 255.0, 221.0 / 255.0),
            ),
            (&mut right_circle, (1.0, 204.0 / 255.0, 68.0 / 255.0)),
        ] {
            object.set_stroke_color(color.0, color.1, color.2, 1.0)?;
            object.set_stroke_width(0.03)?;
        }
        LayoutAnchor::from(&left_circle).surround_circle(&(&left_target).into(), 0, false, 1.2)?;
        scene.add_many(&[
            (&left_target).into(),
            (&left_circle).into(),
            (&right_target).into(),
            (&right_circle).into(),
        ])?;
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.1)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        assert!((live.effective_layout(&right_circle)?.width - 0.8).abs() < 1.0e-6);
        live.surround_circle(
            &(&right_circle).into(),
            &(&right_target).into(),
            1,
            true,
            0.9,
        )?;
        let left = live.effective_layout(&left_circle)?;
        let right = live.effective_layout(&right_circle)?;
        assert!((left.width - 5.0_f64.sqrt() * 1.2).abs() < 1.0e-6);
        assert!((right.height / right.width - 0.5).abs() < 1.0e-6);
        let wait = live.wait_segment(0.1)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(scene.into_execution_with_session(session)?)
    };
    build().map_err(|error| error.to_string())
}
