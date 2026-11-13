//! An absent glow becomes a neutral attachment only when its target animation activates.
//! Paired with web/python/examples/glow_scene_worker_pixels.py on the same shared engine.
use noon::{
    effects::{EffectUpdate, Pixels},
    AnimationOptions, Color, RateFunction, Scene,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut dot = scene.circle(0.4)?;
    dot.disable_stroke()?;
    dot.set_fill(1.0, 1.0, 1.0, 1.0)?;
    scene.add(&dot)?;
    let mut session = scene.into_execution()?;
    let target = session.scene_mut().target_editor(&dot)?;
    session.live().set_glow(
        &target,
        EffectUpdate::default()
            .color(Color::BLUE)
            .radius(Pixels(6.5))
            .intensity(1.4),
    )?;
    assert!(
        dot.get_effect("glow").is_err(),
        "target construction is inert"
    );
    let segment = session.live().declare_and_activate_transform_to(
        &dot,
        &target,
        AnimationOptions::new()
            .run_time(0.25)
            .rate_func(RateFunction::Linear),
    )?;
    let attachment = dot.get_effect("glow")?;
    let neutral = (attachment.authored_definition()?).as_glow().unwrap();
    assert_eq!(neutral.intensity(), 0.0);
    session.live().advance_segment_to(segment, 0.125)?;
    session.live().advance_segment_to(segment, 0.25)?;
    session.live().complete_segment(segment)?;
    let completed = (attachment.authored_definition()?).as_glow().unwrap();
    assert_eq!(completed.intensity(), 1.4);
    session.live().remove_glow(&dot)?;
    assert!(
        attachment.authored_definition().is_err(),
        "generation retired"
    );
    println!("Rust absent-to-glow completion/removal passed at t=0.25");
    Ok(())
}
