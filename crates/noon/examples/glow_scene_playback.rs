//! Persistent glow through public Scene/LiveSession; paired with the Python example.
//! Runs without a Python/browser host. The native raster test qualifies its shared renderer path.
use noon::{
    effects::{EffectDefinition, GlowUpdate, Pixels},
    AnimationOptions, Color, RateFunction, Scene,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut dot = scene.circle(0.4)?;
    dot.disable_stroke()?;
    dot.set_fill(1.0, 1.0, 1.0, 1.0)?;
    dot.set_glow(
        GlowUpdate::default()
            .color(Color::RED)
            .radius(Pixels(3.25))
            .intensity(0.4000000000000123),
    )?;
    scene.add(&dot)?;
    let original = dot.get_effect("glow")?;
    let mut session = scene.execution_session()?;
    let mut live = scene.live(&mut session);
    let target = live.target_editor(&dot)?;
    live.shift(&target, 2.0, 0.0)?;
    live.set_glow(
        &target,
        GlowUpdate::default()
            .color(Color::BLUE)
            .radius(Pixels(6.5))
            .intensity(1.4),
    )?;
    let segment = live.declare_and_activate_transform_to(
        &dot,
        &target,
        AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear),
    )?;
    live.advance_segment_to(segment, 1.0)?;
    live.complete_segment(segment)?;
    let EffectDefinition::Glow(complete) = original.authored_definition()?;
    assert_eq!(complete.intensity(), 1.4);
    assert_eq!(complete.radius(), Pixels(6.5).into());
    assert_eq!(live.effective_layout(&dot)?.center.0, 2.0);

    let neutral = live.target_editor(&dot)?;
    live.set_glow(&neutral, GlowUpdate::default().intensity(0.0))?;
    let fade = live.declare_and_activate_transform_to(
        &dot,
        &neutral,
        AnimationOptions::new()
            .run_time(0.5)
            .rate_func(RateFunction::Linear),
    )?;
    live.advance_segment_to(fade, 1.5)?;
    live.complete_segment(fade)?;
    let EffectDefinition::Glow(complete) = original.authored_definition()?;
    assert_eq!(complete.intensity(), 0.0);
    live.remove_glow(&dot)?;
    assert!(original.authored_definition().is_err());
    let wait = live.wait_segment(0.25)?;
    live.advance_segment_to(wait, 1.75)?;
    live.complete_segment(wait)?;
    assert_eq!(live.effective_layout(&dot)?.center.0, 2.0);
    println!("Rust public glow motion/completion/removal passed at t=1.75");
    Ok(())
}
