//! Temporary glow emphasis with ordinary motion, easing and restoration.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::glow_pulse::program()?)?;
    Ok(())
}
