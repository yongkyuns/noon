fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::zoomed_scene::session()?)?;
    Ok(())
}
