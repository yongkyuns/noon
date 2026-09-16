fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::synchronized_plotting_example::program()?)?;
    Ok(())
}
