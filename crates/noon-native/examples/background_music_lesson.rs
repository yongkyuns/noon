//! Native playback of the same complete lesson as background_music_lesson.py.
//!
//! Run with `cargo run -p noon-native --features audio --example background_music_lesson`.
//! Rust owns the scalar animation, authored sections, and original music.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scene = noon::example_scenes::background_music_lesson::scene()?;
    noon_native::run_with_config(
        scene.execution_session()?,
        noon_native::NativeViewportConfig {
            title: "Noon — Double the input".to_owned(),
            inspection_zoom: true,
            ..Default::default()
        },
    )?;
    Ok(())
}
