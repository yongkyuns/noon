fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::scale_pivots::session()?)?;
    Ok(())
}
