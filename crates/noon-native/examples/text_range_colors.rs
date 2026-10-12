//! Native presentation of the shared Text substring/range color scene.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::text_range_colors::session()?)?;
    Ok(())
}
