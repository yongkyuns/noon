//! Ordinary style animation shares property tracks on native and direct Rust/WASM.
use crate::{
    AnimationCompositionRequest, AnimationOptions, ExecutionSession, RateFunction, Scene,
    TransformToRequest,
};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut shape = scene.circle(1.)?;
        shape.disable_fill()?;
        shape.set_stroke_color(1., 1., 1., 1.)?;
        shape.set_stroke_width(0.)?;
        scene.add(&shape)?;
        let mut session = scene.execution_session()?;
        for width in [0.2, 0.02] {
            let mut live = scene.live(&mut session);
            let target = live.target_editor(&shape)?;
            live.set_style(
                &target,
                crate::StyleUpdate {
                    stroke_width: Some(width),
                    ..Default::default()
                },
            )?;
            let request =
                AnimationCompositionRequest::TransformTo(TransformToRequest::point_correspondence(
                    &shape,
                    &target,
                    AnimationOptions::new()
                        .run_time(1.)
                        .rate_func(RateFunction::Linear),
                ));
            let segment =
                live.declare_and_activate_composition(&request, AnimationOptions::new())?;
            live.advance_segment_to(segment, segment.end_time())?;
            live.complete_segment(segment)?;
        }
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
