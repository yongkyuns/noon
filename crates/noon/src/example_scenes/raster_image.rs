//! Immutable images, late admission, affine motion and fades on one shared program.

use crate::{
    AnimationOptions, ContinuationStep, ImageMobjectOptions, LiveContinuation, LiveProgram,
    LiveSession, Mobject, RasterImageSampling, RateFunction, Scene, SemanticFadeDirection,
};

/// Deliberately asymmetric straight-alpha fixture: top row red / half-green,
/// bottom row transparent blue / white. Shared by native and direct WASM hosts.
pub const PIXELS: [u8; 16] = [
    255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 255, 255, 255, 255,
];

pub fn image_options(height: f64) -> Result<ImageMobjectOptions, String> {
    let mut options = ImageMobjectOptions::rgba8(2, 2, PIXELS.to_vec()).map_err(message)?;
    options.set_height(height).map_err(message)?;
    options.set_sampling(RasterImageSampling::Nearest);
    Ok(options)
}

pub struct RasterImage {
    image: Mobject,
    stage: u8,
}

impl LiveContinuation for RasterImage {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let segment = match self.stage {
            0 => live.declare_and_activate_fade(&self.image, SemanticFadeDirection::In, options),
            1 => {
                // This constructor runs after bootstrap. The resource deduplicates,
                // but the new object stays detached until the ordinary add operation.
                let second = live.create_image(image_options(1.0)?).map_err(message)?;
                if live.contains(&second).map_err(message)? {
                    return Err("new image was admitted before add".into());
                }
                live.set_translation(&second, 2.0, 0.0).map_err(message)?;
                live.add(&second).map_err(message)?;
                let target = live.target_editor(&self.image).map_err(message)?;
                live.set_translation(&target, 0.0, 0.0).map_err(message)?;
                live.rotate(&target, std::f64::consts::FRAC_PI_4)
                    .map_err(message)?;
                live.scale(&target, 0.75, 0.75).map_err(message)?;
                live.set_opacity(&target, 0.6).map_err(message)?;
                live.declare_and_activate_transform_to(&self.image, &target, options)
            }
            2 => live.declare_and_activate_fade(&self.image, SemanticFadeDirection::Out, options),
            3 => {
                if live.contains(&self.image).map_err(message)? {
                    return Err("FadeOut did not remove its image".into());
                }
                live.wait_segment(1.0)
            }
            4 => return Ok(ContinuationStep::Finished),
            _ => return Err("image continuation resumed after completion".into()),
        }
        .map_err(message)?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<RasterImage>, String> {
    let mut scene = Scene::new();
    let mut background = scene.square(8.0).map_err(message)?;
    background
        .set_fill(20.0 / 255.0, 40.0 / 255.0, 60.0 / 255.0, 1.0)
        .map_err(message)?;
    background.disable_stroke().map_err(message)?;
    scene.add(&background).map_err(message)?;
    let mut image = scene.image(image_options(2.0)?).map_err(message)?;
    image.set_translation(-2.0, 0.0).map_err(message)?;
    scene
        .into_live_program(RasterImage { image, stage: 0 })
        .map_err(message)
}

/// Static filter/opacity fixture, also used by native and direct browser hosts.
pub fn sampling_session(
    sampling: RasterImageSampling,
    opacity: f64,
) -> Result<crate::ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut background = scene.square(8.0).map_err(message)?;
    background
        .set_fill(20.0 / 255.0, 40.0 / 255.0, 60.0 / 255.0, 1.0)
        .map_err(message)?;
    background.disable_stroke().map_err(message)?;
    scene.add(&background).map_err(message)?;
    let mut options = image_options(4.0)?;
    options.set_sampling(sampling);
    options.set_opacity(opacity).map_err(message)?;
    let image = scene.image(options).map_err(message)?;
    scene.add(&image).map_err(message)?;
    scene.execution_session().map_err(message)
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn native_image_program_uses_one_resource_across_live_admission_and_removal() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        for (stage, visible) in [2, 3, 3, 2].into_iter().enumerate() {
            assert!(matches!(
                program.resume().unwrap(),
                LiveProgramStatus::Awaiting(_)
            ));
            program
                .drive_to(&mut callbacks, stage as f64 + 0.5)
                .unwrap();
            let session = program.session();
            assert_eq!(
                session
                    .frame()
                    .objects
                    .iter()
                    .filter_map(|row| row.content.image().map(|image| image.resource()))
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                1
            );
            assert_eq!(
                (0..session.frame().objects.len())
                    .filter(|&i| session.frame().is_present(i))
                    .count(),
                visible
            );
            match program
                .drive_to(&mut callbacks, stage as f64 + 1.0)
                .unwrap()
            {
                LiveProgramStatus::PublicationPending(expected) => {
                    let context = program.take_renderer_publication().context();
                    assert_eq!(expected, context);
                    program.admit_publication(context).unwrap();
                }
                LiveProgramStatus::ReadyToResume => {}
                status => panic!("unexpected completion: {status:?}"),
            }
        }
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }
}
