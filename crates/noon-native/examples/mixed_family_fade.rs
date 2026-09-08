//! Paired with web/python/examples/mixed_family_fade.py.
use noon::{
    AnimationOptions, ContinuationStep, LiveContinuation, LiveSession, MobjectFamily, RateFunction,
    Scene, SemanticFadeDirection,
};

struct MixedFade {
    family: MobjectFamily,
    stage: u8,
}

impl LiveContinuation for MixedFade {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let direction = match self.stage {
            0 => SemanticFadeDirection::In,
            1 => SemanticFadeDirection::Out,
            2 => {
                self.stage += 1;
                return live
                    .wait_segment(0.25)
                    .map(ContinuationStep::Await)
                    .map_err(|e| e.to_string());
            }
            _ => return Ok(ContinuationStep::Finished),
        };
        self.stage += 1;
        live.declare_and_activate_family_fade(
            &self.family,
            direction,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear)
                .lag_ratio(0.25),
        )
        .map(ContinuationStep::Await)
        .map_err(|e| e.to_string())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scene = Scene::new();
    let mut circle = scene.circle(0.4)?;
    circle.set_translation(-2.0, 0.0)?;
    let mut square = scene.rectangle(0.8, 0.8)?;
    for object in [&mut circle, &mut square] {
        object.set_color(1.0, 1.0, 1.0, 1.0)?;
        object.set_fill(1.0, 1.0, 1.0, 0.0)?;
    }
    let mut text = scene.text("TEXT")?;
    text.set_translation(2.0, 0.0)?;
    let family = scene.family(&[(&circle).into(), (&square).into(), (&text).into()])?;
    noon_native::run_live_program(scene.into_live_program(MixedFade { family, stage: 0 })?)?;
    Ok(())
}
