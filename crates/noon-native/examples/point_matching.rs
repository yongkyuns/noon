fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::point_matching::session()?)?;
    Ok(())
}
