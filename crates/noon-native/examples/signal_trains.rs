//! Run with `cargo run -p noon-native --example signal_trains`.
//! Staged Rust continuation preserves transient signal membership during playback.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::line_passing_flash::train_program()?)?;
    Ok(())
}
