fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::angle_example::program()?)?;
    Ok(())
}
