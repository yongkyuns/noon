//! High-level visibility lifecycle, paired with web/python/examples/dissolve.py.
use crate::{
    effects::{DissolveDirection, DissolveStyle, EffectUpdate, Glow, GlowSource, Pixels},
    AnimationCompositionRequest, AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram,
    LiveSession, MobjectFamily, RateFunction, Scene,
};

pub struct VisibilityLifecycle {
    source: MobjectFamily,
    stage: u8,
}
fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

impl LiveContinuation for VisibilityLifecycle {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let segment = match self.stage {
            0 | 2 | 4 | 6 | 8 => live.wait_segment(0.5),
            1 | 3 | 5 | 7 => {
                let direction = if self.stage == 1 || self.stage == 5 {
                    DissolveDirection::Out
                } else {
                    DissolveDirection::In
                };
                let style = DissolveStyle::new(EffectUpdate::default().seed(7).softness(0.2))
                    .map_err(message)?;
                let mut options = AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear);
                if self.stage == 1 {
                    options = options.remover(false);
                }
                live.declare_and_activate_composition(
                    &AnimationCompositionRequest::Dissolve {
                        target: (&self.source).into(),
                        direction,
                        style,
                        options,
                    },
                    AnimationOptions::new(),
                )
            }
            9 => return Ok(ContinuationStep::Finished),
            _ => return Err("visibility lifecycle resumed after completion".into()),
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
    let mut family = scene
        .family(&[(&left).into(), (&right).into()])
        .map_err(message)?;
    family
        .add_effect(
            Glow::new(
                EffectUpdate::default()
                    .radius(Pixels(2.0))
                    .source(GlowSource::Silhouette),
            )
            .map_err(message)?,
            "halo",
        )
        .map_err(message)?;
    Ok(family)
}

pub fn program() -> Result<LiveProgram<VisibilityLifecycle>, String> {
    let mut scene = Scene::new();
    let mut background = scene.rectangle(16.0, 9.0).map_err(message)?;
    background.set_fill(0.0, 0.0, 0.0, 1.0).map_err(message)?;
    background.disable_stroke().map_err(message)?;
    scene.add(&background).map_err(message)?;
    let reference = source(&mut scene, -2.0)?;
    let source = source(&mut scene, 2.0)?;
    scene
        .add_many(&[(&reference).into(), (&source).into()])
        .map_err(message)?;
    scene
        .into_live_program(VisibilityLifecycle { source, stage: 0 })
        .map_err(message)
}
