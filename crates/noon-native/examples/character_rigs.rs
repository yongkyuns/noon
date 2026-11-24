//! Native paired demo for the character rig gallery lesson.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scene = noon::example_scenes::character_rigs::scene()?;
    noon_native::run_with_config(
        scene.into_execution()?,
        noon_native::NativeViewportConfig {
            title: "Same move, different reach".into(),
            playback_controls: true,
            ..Default::default()
        },
    )?;
    Ok(())
}
