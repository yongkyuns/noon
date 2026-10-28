//! Authored camera Bloom animates through the ordinary Rust timeline.
use noon::{
    effects::{Bloom, EffectUpdate, Pixels},
    AnimationOptions, RateFunction, Scene,
};
use noon_core::ViewColorProfile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut camera = scene.camera_frame()?;
    camera.set_view_color_profile(ViewColorProfile::LinearHdr)?;
    camera.add_effect(
        Bloom::new(
            EffectUpdate::default()
                .radius(Pixels(12.0))
                .threshold(0.15)
                .intensity(0.0),
        )?,
        "bloom",
    )?;
    let mut circle = scene.circle(1.0)?;
    circle.set_fill(0.7, 0.85, 1.0, 1.0)?;
    circle.disable_stroke()?;
    scene.add(&circle)?;
    let mut target = camera.target_editor()?;
    target.set_effect("bloom", EffectUpdate::default().intensity(3.0))?;
    let animation = scene.declare_transform_to(
        &camera,
        &target,
        AnimationOptions::new()
            .run_time(3.0)
            .rate_func(RateFunction::Linear),
    )?;
    noon_native::run(scene.into_execution_with_animation_root(&animation)?)?;
    Ok(())
}
