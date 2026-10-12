fn main() -> Result<(), Box<dyn std::error::Error>> {
    let program = noon::time_series_plotting_example::program().map_err(std::io::Error::other)?;
    noon_native::run_live_program(program)?;
    Ok(())
}
