//! Material radiance and camera Bloom animate independently, then restore.
use crate::{
    effects::{Bloom, EffectUpdate, Pixels},
    AnimationCompositionRequest, AnimationOptions, ContinuationStep, LiveContinuation, LiveProgram,
    Mobject, RateFunction, Scene, SemanticAnimationCompositionKind,
};
use noon_core::ViewColorProfile;

pub struct CameraBloom {
    camera: Mobject,
    panel: Mobject,
    stage: u8,
}
impl LiveContinuation for CameraBloom {
    type Error = String;
    fn resume(&mut self, scene: &mut crate::ContinuationScene) -> Result<ContinuationStep, String> {
        let segment = match self.stage {
            0 | 2 => scene.live().wait_segment(0.5).map_err(message)?,
            1 | 3 => {
                let bright = self.stage == 1;
                let panel = scene.live().target_editor(&self.panel).map_err(message)?;
                let camera = scene.live().target_editor(&self.camera).map_err(message)?;
                scene
                    .set_emission(&panel, if bright { 7.0 } else { 0.0 })
                    .map_err(message)?;
                scene
                    .live()
                    .set_effect(
                        &camera,
                        "bloom",
                        EffectUpdate::default().intensity(if bright { 3.0 } else { 0.0 }),
                    )
                    .map_err(message)?;
                let options = AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear);
                scene
                    .live()
                    .declare_and_activate_animation_composition(
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
            4 => return Ok(ContinuationStep::Finished),
            _ => return Err("camera Bloom resumed after completion".into()),
        };
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}
fn setup() -> Result<(Scene, CameraBloom), String> {
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
    Ok((
        scene,
        CameraBloom {
            camera,
            panel,
            stage: 0,
        },
    ))
}
pub fn program() -> Result<LiveProgram<CameraBloom>, String> {
    let (scene, continuation) = setup()?;
    scene.into_live_program(continuation).map_err(message)
}
/// Pre-author the same finite steps for normal seek/export playback.
pub fn execution() -> Result<crate::SceneExecution, String> {
    let (mut scene, mut continuation) = setup()?;
    let session = scene.execution_session().map_err(message)?;
    scene.install_execution(session);
    let mut scene = crate::ContinuationScene::new(scene);
    loop {
        match continuation.resume(&mut scene)? {
            ContinuationStep::Finished => break,
            ContinuationStep::Publish(_) => {
                return Err(
                    "camera Bloom pre-authoring cannot prepare custom shader resources".into(),
                );
            }
            ContinuationStep::Await(segment) => {
                let mut live = scene.live();
                live.advance_segment_to(segment, segment.end_time())
                    .map_err(message)?;
                live.complete_segment(segment).map_err(message)?;
            }
        }
    }
    let mut execution = scene.take().into_execution().map_err(message)?;
    execution.seek(0.0).map_err(message)?;
    Ok(execution)
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(all(test, feature = "prepared-prefix"))]
mod prefix_tests {
    use super::*;
    use noon_runtime::ReplayLimits;

    fn bloom_intensity(frame: &noon_runtime::FrameState) -> f64 {
        frame
            .objects
            .iter()
            .filter_map(|row| row.effects.as_deref())
            .flat_map(|stack| stack.iter())
            .find_map(|effect| effect.definition.as_bloom().map(|bloom| bloom.intensity()))
            .expect("the camera owns its authored Bloom")
    }

    #[test]
    fn prepared_prefix_adopts_camera_bloom_effect_history() {
        let (mut scene, mut continuation) = setup().unwrap();
        let source = scene.execution_session().unwrap();
        scene.install_execution(source);
        let mut scene = crate::ContinuationScene::new(scene);
        scene
            .owned_execution_mut()
            .begin_replay_retention(ReplayLimits::default())
            .unwrap();
        let first = match continuation.resume(&mut scene).unwrap() {
            ContinuationStep::Await(segment) => segment,
            other => panic!("expected initial wait, got {other:?}"),
        };
        scene
            .owned_execution_mut()
            .advance_to(first.end_time())
            .unwrap();
        scene.live().complete_segment(first).unwrap();
        let (mut publisher, mut reader) = scene
            .owned_execution()
            .create_prepared_prefix(first)
            .unwrap();

        let mut last = first;
        loop {
            match continuation.resume(&mut scene).unwrap() {
                ContinuationStep::Await(segment) => {
                    scene
                        .owned_execution_mut()
                        .advance_to(segment.end_time())
                        .unwrap();
                    scene.live().complete_segment(segment).unwrap();
                    let suffix = scene
                        .owned_execution()
                        .publish_prepared_prefix(segment, &mut publisher)
                        .expect("authored camera Bloom changes stay in their typed history owner");
                    reader.preflight_append(&suffix).unwrap();
                    reader.append(suffix).unwrap();
                    last = segment;
                }
                ContinuationStep::Finished => break,
                ContinuationStep::Publish(_) => panic!("built-in effects need no host resources"),
            }
        }
        scene.owned_execution_mut().seal_replay().unwrap();
        reader
            .append(
                scene
                    .owned_execution()
                    .finish_prepared_prefix(last, &mut publisher)
                    .unwrap(),
            )
            .unwrap();
        for (time, intensity) in [
            (0.0, 0.0),
            (0.5, 0.0),
            (1.5, 1.5),
            (2.5, 3.0),
            (3.5, 2.25),
            (4.5, 0.75),
            (5.0, 0.0),
        ] {
            scene.owned_execution_mut().seek(time).unwrap();
            reader.seek(time).unwrap();
            assert_eq!(
                reader.frame(),
                scene.owned_execution().frame(),
                "at authored time {time}"
            );
            assert_eq!(bloom_intensity(reader.frame()), intensity);
        }
    }
}
