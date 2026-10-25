//! An image keeps its pixels while the ordinary glow and pose tracks change.
//! Paired with `web/python/examples/image_glow.py`.

use crate::{
    effects::{GlowUpdate, Pixels},
    AnimationOptions, Color, ContinuationStep, ImageMobjectOptions, LiveContinuation, LiveProgram,
    LiveSession, Mobject, RasterImageSampling, RateFunction, Scene,
};

pub const SIDE: u32 = 9;

/// A tiny soft-edged diamond. Hidden magenta texels must remain transparent.
/// This preparation runs once; no host callback generates pixels during playback.
pub fn diamond_rgba8() -> Vec<u8> {
    let mut pixels = Vec::with_capacity((SIDE * SIDE * 4) as usize);
    for y in 0..SIDE as i32 {
        for x in 0..SIDE as i32 {
            let distance = (x - 4).abs() + (y - 4).abs();
            let alpha = match distance {
                0..=3 => 255,
                4 => 160,
                5 => 48,
                _ => 0,
            };
            let rgba = if alpha == 0 {
                [255, 0, 255, 0]
            } else {
                [(100 + 17 * x) as u8, (160 + 10 * y) as u8, 255, alpha]
            };
            pixels.extend_from_slice(&rgba);
        }
    }
    pixels
}

pub fn image_options() -> Result<ImageMobjectOptions, String> {
    let mut options = ImageMobjectOptions::rgba8(SIDE, SIDE, diamond_rgba8()).map_err(message)?;
    options.set_height(2.0).map_err(message)?;
    options.set_sampling(RasterImageSampling::Linear);
    Ok(options)
}

pub struct ImageGlow {
    image: Mobject,
    stage: u8,
}

impl LiveContinuation for ImageGlow {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let motion = AnimationOptions::new()
            .run_time(1.5)
            .rate_func(RateFunction::Smooth);
        let segment = match self.stage {
            0 | 2 | 4 => live.wait_segment(0.75),
            1 => {
                let target = live.target_editor(&self.image).map_err(message)?;
                live.shift(&target, 0.0, 0.6).map_err(message)?;
                live.rotate(&target, std::f64::consts::PI / 12.0)
                    .map_err(message)?;
                live.set_glow(
                    &target,
                    GlowUpdate::default()
                        .color(Color::PINK)
                        .radius(Pixels(20.0))
                        .intensity(2.2),
                )
                .map_err(message)?;
                live.declare_and_activate_transform_to(&self.image, &target, motion)
            }
            3 => {
                // Return to the original pose while making the effect neutral.
                let target = live.target_editor(&self.image).map_err(message)?;
                live.shift(&target, 0.0, -0.6).map_err(message)?;
                live.rotate(&target, -std::f64::consts::PI / 12.0)
                    .map_err(message)?;
                live.set_glow(
                    &target,
                    GlowUpdate::default()
                        .color(Color::BLUE)
                        .radius(Pixels(10.0))
                        .intensity(0.0),
                )
                .map_err(message)?;
                live.declare_and_activate_transform_to(&self.image, &target, motion)
            }
            5 => {
                // Removing a neutral effect preserves the same ordinary image.
                live.remove_glow(&self.image).map_err(message)?;
                live.wait_segment(0.75)
            }
            6 => return Ok(ContinuationStep::Finished),
            _ => return Err("image glow continuation resumed after completion".into()),
        }
        .map_err(message)?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<ImageGlow>, String> {
    let mut scene = Scene::new();
    let mut background = scene.rectangle(16.0, 9.0).map_err(message)?;
    background
        .set_fill(8.0 / 255.0, 16.0 / 255.0, 31.0 / 255.0, 1.0)
        .map_err(message)?;
    background.disable_stroke().map_err(message)?;
    scene.add(&background).map_err(message)?;

    // Both objects share one immutable image resource; the left is a reference.
    let options = image_options()?;
    let mut reference = scene.image(options.clone()).map_err(message)?;
    reference.set_translation(-2.2, 0.0).map_err(message)?;
    scene.add(&reference).map_err(message)?;
    let mut image = scene.image(options).map_err(message)?;
    image.set_translation(2.2, 0.0).map_err(message)?;
    image
        .set_glow(
            GlowUpdate::default()
                .color(Color::BLUE)
                .radius(Pixels(10.0))
                .intensity(1.4),
        )
        .map_err(message)?;
    scene.add(&image).map_err(message)?;
    scene
        .into_live_program(ImageGlow { image, stage: 0 })
        .map_err(message)
}

fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
