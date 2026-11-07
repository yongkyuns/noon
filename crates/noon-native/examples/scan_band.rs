fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::scan_band::execution()?)?;
    Ok(())
}
