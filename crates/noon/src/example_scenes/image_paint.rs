//! One immutable texture becomes a crop, a contained image, a stretched fill,
//! and a moving repeat pattern through ordinary shared fill operations.
//! Paired with `web/python/examples/image_paint.py`.
use crate::{
    AnimationOptions, ContinuationStep, ImageAddressMode, ImageFit, ImageMapping,
    ImageMobjectOptions, ImagePaint, ImageUvTransform, LiveContinuation, LiveProgram, LiveSession,
    Mobject, RasterImageSampling, RateFunction, Scene,
};

/// Top-first, asymmetric colors include partial alpha and hidden colored texels.
pub const TEXTURE_RGBA8: [u8; 64] = [
    255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 255, 255, 0, 255, 0, 255, 255, 255, 255, 0, 255,
    64, 255, 128, 0, 255, 32, 96, 224, 192, 128, 0, 255, 255, 255, 255, 255, 0, 0, 128, 255, 128,
    255, 32, 96, 255, 16, 160, 80, 255, 96, 48, 192, 255, 224, 224, 224, 128, 0, 64, 192, 255,
];

pub struct TexturedSurface {
    panel: Mobject,
    source: Mobject,
    stage: u8,
}

fn paint(source: &Mobject, profile: u8) -> Result<ImagePaint, String> {
    let fit = match profile {
        0 => ImageFit::Cover,
        1 => ImageFit::Contain,
        _ => ImageFit::Stretch,
    };
    let address = if profile >= 3 {
        ImageAddressMode::Repeat
    } else {
        ImageAddressMode::Clamp
    };
    let transform = match profile {
        3 => ImageUvTransform::new([0.0, 0.0], [0.5, 0.5], 0.0),
        4 => ImageUvTransform::new([0.2, -0.15], [0.75, 0.6], std::f64::consts::FRAC_PI_4),
        _ => Ok(ImageUvTransform::default()),
    }
    .map_err(message)?;
    let mapping = ImageMapping::new(fit, [0.5, 0.5], address, transform).map_err(message)?;
    source
        .image_paint(mapping, Some(RasterImageSampling::Nearest))
        .map_err(message)
}

impl LiveContinuation for TexturedSurface {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let segment = match self.stage {
            0 | 5 => live.wait_segment(0.5),
            1..=3 => {
                // Fit and address are discrete choices; pixels remain shared.
                live.set_fill_image(&self.panel, paint(&self.source, self.stage)?, None)
                    .map_err(message)?;
                live.wait_segment(0.5)
            }
            4 => {
                // Placement and opacity share the ordinary target animation.
                let target = live.target_editor(&self.panel).map_err(message)?;
                live.set_fill_image(&target, paint(&self.source, 4)?, Some(0.5))
                    .map_err(message)?;
                live.shift(&target, 0.5, 0.25).map_err(message)?;
                live.declare_and_activate_transform_to(
                    &self.panel,
                    &target,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                )
            }
            6 => {
                // Removing the image paint is an immediate ordinary fill edit.
                live.set_fill(&self.panel, 0.25, 0.5, 0.75, 1.0)
                    .map_err(message)?;
                live.wait_segment(0.5)
            }
            7 => return Ok(ContinuationStep::Finished),
            _ => return Err("textured surface resumed after completion".into()),
        }
        .map_err(message)?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<TexturedSurface>, String> {
    let mut scene = Scene::new();
    let options = ImageMobjectOptions::rgba8(4, 4, TEXTURE_RGBA8.as_slice()).map_err(message)?;
    // This source supplies an existing resource without adding a visible object.
    let source = scene.image(options).map_err(message)?;
    let mut panel =
        Mobject::manim_rectangle(std::rc::Rc::clone(scene.integration_store()), 4.0, 2.0)
            .map_err(message)?;
    panel.disable_stroke().map_err(message)?;
    panel
        .set_fill_image(paint(&source, 0)?, Some(1.0))
        .map_err(message)?;
    scene.add(&panel).map_err(message)?;
    scene
        .into_live_program(TexturedSurface {
            panel,
            source,
            stage: 0,
        })
        .map_err(message)
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
