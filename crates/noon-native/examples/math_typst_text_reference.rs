//! Native presentation of the shared static MathTypst reference scene.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::math_typst_text_reference()?)?;
    Ok(())
}
