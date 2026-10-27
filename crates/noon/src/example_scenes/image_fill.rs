//! A reusable image surface with authored-time UV and opacity animation.
//! Paired with web/python/examples/image_fill.py on the ordinary engine path.
use crate::{
    AnimationOptions, ContinuationStep, ImageAddressMode, ImageFit, ImageMapping,
    ImagePaintOptions, ImageUvTransform, LiveContinuation, LiveProgram, LiveSession, Mobject,
    RateFunction, Scene,
};

pub struct ImageSurface {
    panel: Mobject,
    stage: u8,
}

pub fn pixels() -> Vec<u8> {
    vec![
        255, 0, 0, 255, 0, 255, 255, 0, 0, 0, 255, 128, 240, 100, 10, 64, 20, 240, 100, 192, 150,
        20, 240, 255,
    ]
}

fn mapping(mirrored: bool) -> Result<ImageMapping, String> {
    ImageMapping::new(
        ImageFit::Cover,
        [0.5; 2],
        ImageAddressMode::Clamp,
        ImageUvTransform::new(
            if mirrored {
                [-0.23, 0.17]
            } else {
                [0.173, -0.231]
            },
            if mirrored {
                [-0.71, 0.58]
            } else {
                [0.63, 1.37]
            },
            if mirrored { -0.61 } else { 0.37 },
        )
        .map_err(message)?,
    )
    .map_err(message)
}

impl LiveContinuation for ImageSurface {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let segment = match self.stage {
            0 => live.wait_segment(0.5),
            1 => {
                let target = live.target_editor(&self.panel).map_err(message)?;
                live.set_fill_image_mapping(&target, mapping(false)?, Some(0.5))
                    .map_err(message)?;
                live.declare_and_activate_transform_to(
                    &self.panel,
                    &target,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                )
            }
            2 => live.wait_segment(0.25),
            3 => {
                // Reflection is a discrete edit; animation never crosses a zero UV scale.
                live.set_fill_image_mapping(&self.panel, mapping(true)?, Some(0.8))
                    .map_err(message)?;
                live.wait_segment(0.25)
            }
            4 => {
                live.set_fill(&self.panel, 0.2, 0.4, 0.8, 1.0)
                    .map_err(message)?;
                live.wait_segment(0.25)
            }
            5 => return Ok(ContinuationStep::Finished),
            _ => return Err("image surface resumed after completion".into()),
        }
        .map_err(message)?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<ImageSurface>, String> {
    let mut scene = Scene::new();
    let mut panel =
        Mobject::manim_rectangle(std::rc::Rc::clone(scene.integration_store()), 4.0, 2.0)
            .map_err(message)?;
    panel.disable_stroke().map_err(message)?;
    panel
        .set_fill_image(
            ImagePaintOptions::rgba8(3, 2, pixels()).map_err(message)?,
            Some(1.0),
        )
        .map_err(message)?;
    scene.add(&panel).map_err(message)?;
    scene
        .into_live_program(ImageSurface { panel, stage: 0 })
        .map_err(message)
}
fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// Static finite-profile corpus used by native/direct-WASM and Python oracles.
/// The index selects authored parameters only; execution remains the normal Scene.
pub fn profile_scene(index: u32) -> Result<Scene, String> {
    if index >= 27 {
        return Err("image paint profile must be in 0..27".into());
    }
    let fit = [ImageFit::Stretch, ImageFit::Cover, ImageFit::Contain][(index / 9) as usize];
    let address = [
        ImageAddressMode::Transparent,
        ImageAddressMode::Clamp,
        ImageAddressMode::Repeat,
    ][((index / 3) % 3) as usize];
    let sampling = [
        crate::RasterImageSampling::Nearest,
        crate::RasterImageSampling::Linear,
        crate::RasterImageSampling::Bicubic,
    ][(index % 3) as usize];
    let mut scene = Scene::new();
    let mut panel =
        Mobject::manim_rectangle(std::rc::Rc::clone(scene.integration_store()), 4.0, 2.0)
            .map_err(message)?;
    panel.disable_stroke().map_err(message)?;
    let mut options = ImagePaintOptions::rgba8(3, 2, pixels()).map_err(message)?;
    options.set_sampling(sampling);
    options.set_mapping(
        ImageMapping::new(
            fit,
            [0.23, 0.71],
            address,
            ImageUvTransform::new([0.173, -0.231], [-0.71, 0.58], 0.37).map_err(message)?,
        )
        .map_err(message)?,
    );
    panel.set_fill_image(options, Some(0.7)).map_err(message)?;
    panel.set_object_opacity(0.6).map_err(message)?;
    scene.add(&panel).map_err(message)?;
    Ok(scene)
}
