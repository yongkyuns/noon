fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::linear_gradient::program()?)?;
    Ok(())
}
