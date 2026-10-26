//! Native presentation of the shared light/dark preset scene.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::presentation_presets::session()?)?;
    Ok(())
}
