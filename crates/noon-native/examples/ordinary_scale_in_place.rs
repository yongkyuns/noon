fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::scale_in_place::program()?)?;
    Ok(())
}
