//! Runnable declaration ownership and unsupported-stack contract. No Python or renderer required.
use noon::effects::{EffectDefinition, Glow, GlowUpdate, Pixels};
use noon::{AnimationOptions, Scene};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let dot = scene.circle(0.08)?;
    scene.add(&dot)?;
    scene.set_glow(&dot, GlowUpdate::default().intensity(0.25))?;
    scene.add_effect(
        &dot,
        Glow::new(GlowUpdate::default().radius(Pixels(12.0)))?,
        "accent",
    )?;
    let original = scene.get_effect(&dot, "glow")?;

    let mut target = dot.target_editor()?;
    target.shift(2.0, 0.0)?;
    target.set_glow(GlowUpdate::default().intensity(1.4))?;
    target.set_effect("accent", GlowUpdate::default().intensity(0.6))?;
    let movement =
        scene.declare_transform_to(&dot, &target, AnimationOptions::new().run_time(1.5))?;
    assert_eq!(movement.options()?.run_time, Some(1.5));
    let EffectDefinition::Glow(base) = original.authored_definition()?;
    assert_eq!(base.intensity(), 0.25);
    assert_ne!(original.node_id(), target.get_effect("glow")?.node_id());

    // Multiple attachments and this unfilled source are outside the finite profile.
    // Reject the reachable source instead of silently dropping its appearance.
    assert!(scene.execution_session().is_err());
    scene.remove_effect(&dot, &original)?;
    assert!(original.authored_definition().is_err());
    let EffectDefinition::Glow(independent) = target.get_effect("glow")?.authored_definition()?;
    assert_eq!(independent.intensity(), 1.4);
    scene.remove_effect(&dot, "accent")?;
    target.remove_glow()?;
    target.remove_effect("accent")?;
    // As in the Python pair, rejection must not poison ordinary continuation.
    scene.wait(0.1)?;
    assert_eq!(scene.time(), 0.1);
    assert!(scene.execution_session().is_ok());
    println!("Rust declaration/copy checks passed; unsupported stack rejected.");
    Ok(())
}
