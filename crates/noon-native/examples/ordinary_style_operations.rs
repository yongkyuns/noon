fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::style_operations::session()?)?;
    Ok(())
}
