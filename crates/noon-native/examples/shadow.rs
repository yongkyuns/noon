//! Move an ordinary image while its colored soft shadow changes direction.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::shadow::program()?)?;
    Ok(())
}
