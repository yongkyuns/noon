//! Treat an isolated family as one composition through the ordinary native host.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::composed_effects::program()?)?;
    Ok(())
}
