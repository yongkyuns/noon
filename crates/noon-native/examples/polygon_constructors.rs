fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::polygon_example::program()?)?;
    Ok(())
}
