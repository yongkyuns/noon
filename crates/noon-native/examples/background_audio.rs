//! Native playback of the same finite score and animation as direct Rust/WASM.
//!
//! Run with `cargo run -p noon-native --features audio --example background_audio`.
//! Preparation is silent. Playback begins when the native picture is ready;
//! closing the window retires output. The shared scene buttons and native keys
//! control the same admitted playback session.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (scene, animation) = noon::example_scenes::background_audio::scene()?;
    let session = scene.execution_session_with_animation_root(&animation)?;
    noon_native::run_with_config(
        session,
        noon_native::NativeViewportConfig {
            title: "Noon — A Small Discovery".to_owned(),
            inspection_zoom: true,
            playback_controls: true,
            section_navigation: scene.section_navigation(),
            ..Default::default()
        },
    )?;
    Ok(())
}
