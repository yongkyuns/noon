fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::path_queries::session()?)?;
    Ok(())
}
