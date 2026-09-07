//! Typed live geometry and matcher construction on the native host.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::live_geometry_construction::program()?)?;
    Ok(())
}
