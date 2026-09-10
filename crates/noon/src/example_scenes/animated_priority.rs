//! Method-target priority switches at completion on the ordinary typed runtime.
use crate::{
    AnimationCompositionRequest as Request, AnimationOptions, ExecutionSession, RateFunction,
    Scene, TransformToRequest,
};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut back = scene.square(2.0)?;
        back.set_fill(
            f64::from(noon_core::BLUE.red),
            f64::from(noon_core::BLUE.green),
            f64::from(noon_core::BLUE.blue),
            1.0,
        )?;
        back.set_stroke_width(0.0)?;
        let mut front = scene.square(2.0)?;
        front.set_fill(
            f64::from(noon_core::RED.red),
            f64::from(noon_core::RED.green),
            f64::from(noon_core::RED.blue),
            1.0,
        )?;
        front.set_stroke_width(0.0)?;
        front.shift(0.5, 0.5)?;
        scene.add(&back)?;
        scene.add(&front)?;
        let mut session = scene.execution_session()?;
        for priority in [2.0, -1.0] {
            let mut live = scene.live(&mut session);
            let target = live.target_editor(&back)?;
            live.set_z_index(&(&target).into(), priority, true)?;
            let request = Request::TransformTo(
                TransformToRequest::new(
                    &back,
                    &target,
                    AnimationOptions::new()
                        .run_time(0.5)
                        .rate_func(RateFunction::Linear),
                )
                .method_target(),
            );
            let segment =
                live.declare_and_activate_composition(&request, AnimationOptions::new())?;
            live.advance_segment_to(segment, segment.end_time())?;
            live.complete_segment(segment)?;
            let wait = live.wait_segment(0.25)?;
            live.advance_segment_to(wait, wait.end_time())?;
            live.complete_segment(wait)?;
        }
        // A short child of AnimationGroup finishes with the group, not at its
        // interpolation endpoint. The other child makes that distinction visible.
        let mut live = scene.live(&mut session);
        let back_target = live.target_editor(&back)?;
        live.set_z_index(&(&back_target).into(), 2.0, true)?;
        let front_target = live.target_editor(&front)?;
        live.shift(&front_target, -1.0, 0.0)?;
        let request = Request::Composition {
            kind: crate::SemanticAnimationCompositionKind::Parallel,
            options: AnimationOptions::new().rate_func(RateFunction::Linear),
            children: vec![
                Request::TransformTo(
                    TransformToRequest::new(
                        &back,
                        &back_target,
                        AnimationOptions::new()
                            .run_time(0.5)
                            .rate_func(RateFunction::Linear),
                    )
                    .method_target(),
                ),
                Request::TransformTo(
                    TransformToRequest::new(
                        &front,
                        &front_target,
                        AnimationOptions::new()
                            .run_time(1.5)
                            .rate_func(RateFunction::Linear),
                    )
                    .method_target(),
                ),
            ],
        };
        let segment = live.declare_and_activate_composition(&request, AnimationOptions::new())?;
        live.advance_segment_to(segment, segment.end_time())?;
        live.complete_segment(segment)?;
        let wait = live.wait_segment(0.25)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
