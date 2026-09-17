//! Native presentation of the shared MarkupText reference scene.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::markup_text::session()?)?;
    Ok(())
}
