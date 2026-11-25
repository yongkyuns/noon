//! Image holes and overlapping family coverage, paired with radial_reveal_composed.py.
use crate::{
    effects::{EffectUpdate, RadialEdge},
    AnimationOptions, Color, ImageMobjectOptions, LiveProgram, RadialReveal, RasterImageSampling,
    RateFunction, Scene, SceneScript, ScriptError, Vec2,
};

pub fn image_pixels() -> Vec<u8> {
    let mut pixels = Vec::new();
    for y in 0..4 {
        for x in 0..4 {
            let center = (1..=2).contains(&x) && (1..=2).contains(&y);
            let corner = (x == 0 || x == 3) && (y == 0 || y == 3);
            pixels.extend(if center {
                [255, 0, 255, 0]
            } else {
                [51, 153, 255, if corner { 128 } else { 255 }]
            });
        }
    }
    pixels
}

pub fn program() -> Result<LiveProgram<SceneScript>, ScriptError> {
    let mut scene = Scene::new();
    let mut image_options = ImageMobjectOptions::rgba8(4, 4, image_pixels())?;
    image_options.set_height(1.6)?;
    image_options.set_sampling(RasterImageSampling::Nearest);
    let image = scene.image(image_options)?;
    scene.shift(&image, Vec2::new(-2.0, 0.0))?;
    let left = scene.rectangle(2.0, 1.4)?;
    let right = scene.rectangle(2.0, 1.4)?;
    for (panel, x, color) in [
        (&left, 1.7, Color::rgb(0.2, 0.4, 0.9)),
        (&right, 2.3, Color::rgb(0.95, 0.4, 0.1)),
    ] {
        scene.shift(panel, Vec2::new(x, 0.0))?;
        scene.set_fill(panel, color, 0.55)?;
        scene.disable_stroke(panel)?;
    }
    let group = scene.family(&[(&left).into(), (&right).into()])?;
    scene.into_script(move |scene| async move {
        let timing = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        scene
            .play_with(
                (
                    RadialReveal::new(&image),
                    RadialReveal::new(&group)
                        .edge(RadialEdge::Burn)
                        .parameters(EffectUpdate::default().seed(7)),
                ),
                timing,
            )
            .await?;
        scene.wait(0.25).await?;
        let kept = AnimationOptions::new().remover(false);
        scene
            .play_with(
                (
                    RadialReveal::new(&image)
                        .out()
                        .edge(RadialEdge::Burn)
                        .parameters(EffectUpdate::default().seed(7))
                        .with_options(kept),
                    RadialReveal::new(&group)
                        .out()
                        .edge(RadialEdge::Clean)
                        .with_options(kept),
                    image
                        .animate()
                        .shift(Vec2::new(0.0, 0.5))
                        .set_object_opacity(0.6),
                    group
                        .animate()
                        .shift(Vec2::new(0.0, 0.5))
                        .set_object_opacity(0.6),
                ),
                timing,
            )
            .await?;
        scene.wait(0.25).await?;
        Ok(())
    })
}
