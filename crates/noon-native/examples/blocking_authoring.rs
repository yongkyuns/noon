fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_scene(noon::example_scenes::blocking_authoring::construct)?;
    Ok(())
}
