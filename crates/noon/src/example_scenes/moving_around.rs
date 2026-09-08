//! Direct counterpart of the paired Python MovingAround gallery example.
use crate::{
    AnimationCompositionRequest, AnimationOptions, Color, ContinuationStep, LiveContinuation,
    LiveProgram, LiveSession, Mobject, RateFunction, Scene, TransformToRequest,
};

pub struct MovingAround {
    square: Mobject,
    stage: u8,
}

impl LiveContinuation for MovingAround {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        if self.stage == 4 {
            return Ok(ContinuationStep::Finished);
        }
        let target = live
            .target_editor(&self.square)
            .map_err(|e| e.to_string())?;
        match self.stage {
            0 => live.shift(&target, -1.0, 0.0),
            1 => live.set_fill_color(
                &target,
                f64::from(Color::ORANGE.red),
                f64::from(Color::ORANGE.green),
                f64::from(Color::ORANGE.blue),
                1.0,
            ),
            2 => live.scale(&target, 0.3, 0.3),
            3 => live.set_rotation(&target, 0.4),
            _ => unreachable!(),
        }
        .map_err(|e| e.to_string())?;
        self.stage += 1;
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Smooth);
        let request = if self.stage == 4 {
            // .animate.rotate interpolates points rather than an angular path.
            TransformToRequest::point_correspondence(&self.square, &target, options)
        } else {
            TransformToRequest::new(&self.square, &target, options)
        };
        live.declare_and_activate_composition(
            &AnimationCompositionRequest::TransformTo(request),
            AnimationOptions::new(),
        )
        .map(ContinuationStep::Await)
        .map_err(|e| e.to_string())
    }
}

pub fn program() -> Result<LiveProgram<MovingAround>, String> {
    let scene = Scene::new();
    let mut square = scene.square(2.0)?;
    square.set_color(
        f64::from(Color::BLUE.red),
        f64::from(Color::BLUE.green),
        f64::from(Color::BLUE.blue),
        1.0,
    )?;
    square.set_fill_opacity(1.0)?;
    scene
        .into_live_program(MovingAround { square, stage: 0 })
        .map_err(|e| e.to_string())
}
