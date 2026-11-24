//! Animate independent named treatments through the ordinary native host.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::ordered_effects::program()?)?;
    Ok(())
}
