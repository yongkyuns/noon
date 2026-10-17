//! Run with `cargo run -p noon-native --example signal_trains`.
//! The example composes existing shared Rust PassingFlash animations.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::line_passing_flash::train_session()?)?;
    Ok(())
}
