//! Native playback of the same finite score and animation as direct Rust/WASM.
//!
//! Run with `cargo run -p noon-native --features audio --example background_audio`.
//! Preparation is silent. Playback begins when the native picture is ready;
//! closing the window retires output. This profile has no playback buttons.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (scene, animation) =
        noon::example_scenes::background_audio::scene_without_playback_controls()?;
    let session = scene.execution_session_with_animation_root(&animation)?;
    noon_native::run_with_config(
        session,
        noon_native::NativeViewportConfig {
            title: "Noon — A Small Discovery".to_owned(),
            inspection_zoom: true,
            ..Default::default()
        },
    )?;
    Ok(())
}
