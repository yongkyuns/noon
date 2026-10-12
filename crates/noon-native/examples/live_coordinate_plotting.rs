fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::live_coordinate_plotting_example::program()?)?;
    Ok(())
}
