//! Native presentation of the shared static Typst reference scene.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::typst_text_reference()?)?;
    Ok(())
}
