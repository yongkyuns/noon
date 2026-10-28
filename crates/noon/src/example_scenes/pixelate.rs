//! Square-cell detail lesson paired with web/python/examples/pixelate.py.
use crate::{
    effects::{EffectUpdate, Pixelate, Pixels},
    AnimationCompositionRequest, AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram,
    LiveSession, MobjectFamily, RateFunction, Scene,
};
pub struct PixelDetail {
    source: MobjectFamily,
    stage: u8,
}
fn message(e: impl std::fmt::Display) -> String {
    e.to_string()
}
impl LiveContinuation for PixelDetail {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let segment = match self.stage {
            0 | 2 => live.wait_segment(0.5),
            1 => {
                let target = live.copy_family(&self.source).map_err(message)?;
                live.set_effect(
                    target.root(),
                    "detail",
                    EffectUpdate::default().size(Pixels(1.0)),
                )
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
            3 | 4 => {
                let update = if self.stage == 3 {
                    EffectUpdate::default().size(Pixels(8.0))
                } else {
                    EffectUpdate::default().size(0.2)
                };
                live.set_effect(&self.source, "detail", update)
                    .map_err(message)?;
                live.wait_segment(0.5)
            }
            5 => {
                live.remove_effect(&self.source, "detail")
                    .map_err(message)?;
                live.wait_segment(0.5)
            }
            6 => return Ok(ContinuationStep::Finished),
            _ => return Err("pixel detail resumed after completion".into()),
        }
        .map_err(message)?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}
fn source(scene: &mut Scene, x: f64) -> Result<MobjectFamily, String> {
    let mut a = scene.rectangle(1.2, 0.8).map_err(message)?;
    let mut b = scene.rectangle(1.2, 0.8).map_err(message)?;
    a.set_fill(1.0, 0.0, 0.0, 0.6).map_err(message)?;
    b.set_fill(0.0, 0.0, 1.0, 0.5).map_err(message)?;
    for (v, dx) in [(&mut a, -0.2), (&mut b, 0.2)] {
        v.disable_stroke().map_err(message)?;
        v.set_translation(x + dx, 0.0).map_err(message)?;
    }
    let group = scene.family(&[(&a).into(), (&b).into()]).map_err(message)?;
    group.set_object_opacity(0.75).map_err(message)?;
    Ok(group)
}
pub fn program() -> Result<LiveProgram<PixelDetail>, String> {
    let mut scene = Scene::new();
    scene
        .camera_frame()
        .map_err(message)?
        .set_scale(1.125, 1.125)
        .map_err(message)?;
    let mut background = scene.rectangle(16.0, 9.0).map_err(message)?;
    background
        .set_fill(24.0 / 255.0, 48.0 / 255.0, 64.0 / 255.0, 1.0)
        .map_err(message)?;
    background.disable_stroke().map_err(message)?;
    scene.add(&background).map_err(message)?;
    let reference = source(&mut scene, -2.0)?;
    let mut source = source(&mut scene, 2.0)?;
    source
        .add_effect(Pixelate::default(), "detail")
        .map_err(message)?;
    scene
        .add_many(&[(&reference).into(), (&source).into()])
        .map_err(message)?;
    scene
        .into_live_program(PixelDetail { source, stage: 0 })
        .map_err(message)
}
