fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::apply_matrix::program()?)?;
    Ok(())
}
