//! Persistent composed masks, paired with web/python/examples/noise_dissolve.py.
use crate::{
    effects::{EffectUpdate, NoiseDissolve},
    AnimationCompositionRequest, AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram,
    LiveSession, MobjectFamily, RateFunction, Scene,
};

pub struct ComposedMask {
    source: MobjectFamily,
    stage: u8,
}
fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

impl LiveContinuation for ComposedMask {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let segment = match self.stage {
            0 | 2 | 4 => live.wait_segment(0.5),
            1 | 3 => {
                let target = live.copy_family(&self.source).map_err(message)?;
                let update = if self.stage == 1 {
                    EffectUpdate::default().progress(1.0)
                } else {
                    EffectUpdate::default().progress(0.0).softness(0.4)
                };
                live.set_effect(target.root(), "mask", update)
                    .map_err(message)?;
                live.declare_and_activate_composition(
                    &AnimationCompositionRequest::FamilyMethodTargetTo {
                        source: &self.source,
                        target_state: target.root(),
                        options: AnimationOptions::new()
                            .run_time(1.0)
                            .rate_func(RateFunction::Linear),
                    },
                    AnimationOptions::new(),
                )
            }
            5 => {
                live.set_effect(
                    &self.source,
                    "mask",
                    EffectUpdate::default().seed(u32::MAX).progress(0.5),
                )
                .map_err(message)?;
                live.wait_segment(0.5)
            }
            6 => {
                live.remove_effect(&self.source, "mask").map_err(message)?;
                live.wait_segment(0.5)
            }
            7 => return Ok(ContinuationStep::Finished),
            _ => return Err("composed mask resumed after completion".into()),
        }
        .map_err(message)?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

fn source(scene: &mut Scene, x: f64) -> Result<MobjectFamily, String> {
    let mut left = scene.rectangle(1.2, 0.8).map_err(message)?;
    let mut right = scene.rectangle(1.2, 0.8).map_err(message)?;
    for (object, dx) in [(&mut left, -0.2), (&mut right, 0.2)] {
        object.set_fill(1.0, 1.0, 1.0, 1.0).map_err(message)?;
        object.disable_stroke().map_err(message)?;
        object.set_object_opacity(0.6).map_err(message)?;
        object.set_translation(x + dx, 0.0).map_err(message)?;
    }
    scene
        .family(&[(&left).into(), (&right).into()])
        .map_err(message)
}

pub fn program() -> Result<LiveProgram<ComposedMask>, String> {
    let mut scene = Scene::new();
    let mut background = scene.rectangle(16.0, 9.0).map_err(message)?;
    background.set_fill(0.0, 0.0, 0.0, 1.0).map_err(message)?;
    background.disable_stroke().map_err(message)?;
    scene.add(&background).map_err(message)?;
    let reference = source(&mut scene, -2.0)?;
    let mut source = source(&mut scene, 2.0)?;
    source
        .add_effect(NoiseDissolve::new(7, 0.08).map_err(message)?, "mask")
        .map_err(message)?;
    scene
        .add_many(&[(&reference).into(), (&source).into()])
        .map_err(message)?;
    scene
        .into_live_program(ComposedMask { source, stage: 0 })
        .map_err(message)
}
