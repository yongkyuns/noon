//! Material radiance and camera Bloom animate independently, then restore.
use crate::{
    effects::{Bloom, EffectUpdate, Pixels},
    AnimationCompositionRequest, AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram,
    LiveSession, Mobject, RateFunction, Scene, SemanticAnimationCompositionKind,
};
use noon_core::ViewColorProfile;

pub struct CameraBloom {
    camera: Mobject,
    panel: Mobject,
    stage: u8,
}
impl LiveContinuation for CameraBloom {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let segment = match self.stage {
            0 | 2 | 4 => live.wait_segment(0.5).map_err(message)?,
            1 | 3 => {
                let bright = self.stage == 1;
                let panel = live.target_editor(&self.panel).map_err(message)?;
                let camera = live.target_editor(&self.camera).map_err(message)?;
                live.set_emission(&panel, if bright { 7.0 } else { 0.0 })
                    .map_err(message)?;
                live.set_effect(
                    &camera,
                    "bloom",
                    EffectUpdate::default().intensity(if bright { 3.0 } else { 0.0 }),
                )
                .map_err(message)?;
                let options = AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear);
                live.declare_and_activate_animation_composition(
                    SemanticAnimationCompositionKind::Parallel,
                    &[
                        AnimationCompositionRequest::TransformTo(
                            crate::TransformToRequest::new(&self.panel, &panel, options)
                                .method_target(),
                        ),
                        AnimationCompositionRequest::TransformTo(
                            crate::TransformToRequest::new(&self.camera, &camera, options)
                                .method_target(),
                        ),
                    ],
                    AnimationOptions::new(),
                    AnimationOptions::new(),
                )
                .map_err(message)?
            }
            5 => return Ok(ContinuationStep::Finished),
            _ => return Err("camera Bloom resumed after completion".into()),
        };
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}
pub fn program() -> Result<LiveProgram<CameraBloom>, String> {
    let mut scene = Scene::new();
    let mut camera = scene.camera_frame().map_err(message)?;
    camera
        .set_view_color_profile(ViewColorProfile::LinearHdr)
        .map_err(message)?;
    camera
        .add_effect(
            Bloom::new(
                EffectUpdate::default()
                    .radius(Pixels(4.0))
                    .threshold(0.15)
                    .intensity(0.0),
            )
            .map_err(message)?,
            "bloom",
        )
        .map_err(message)?;
    let mut panel = scene.circle(1.0).map_err(message)?;
    panel.set_fill(0.2, 0.4, 0.8, 1.0).map_err(message)?;
    panel.disable_stroke().map_err(message)?;
    scene.add(&panel).map_err(message)?;
    scene
        .into_live_program(CameraBloom {
            camera,
            panel,
            stage: 0,
        })
        .map_err(message)
}
fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
