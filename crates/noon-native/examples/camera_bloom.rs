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
    noon_native::run_live_program(scene.into_live_program(BloomAnimation {
        animation,
        started: false,
    })?)?;
    Ok(())
}
struct BloomAnimation {
    animation: noon::DeclaredAnimation,
    started: bool,
}
impl noon::LiveContinuation for BloomAnimation {
    type Error = String;
    fn resume(
        &mut self,
        live: &mut noon::LiveSession<'_>,
    ) -> Result<noon::ContinuationStep, String> {
        if self.started {
            return Ok(noon::ContinuationStep::Finished);
        }
        self.started = true;
        live.play_animation(&self.animation)
            .map(noon::ContinuationStep::Await)
            .map_err(|e| e.to_string())
    }
}
