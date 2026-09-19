fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::animated_number_line_example::program()?)?;
    Ok(())
}
