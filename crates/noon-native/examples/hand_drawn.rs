//! Run the seekable hand-drawn diagram with the native Rust viewer.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::hand_drawn::program()?)?;
    Ok(())
}
