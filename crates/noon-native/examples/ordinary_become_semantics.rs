//! Shared flagged-become semantics on the native host.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::ordinary_become_semantics::program()?)?;
    Ok(())
}
