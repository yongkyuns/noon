//! Direct native Rust example, paired with the ordinary Python create_shapes scene.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::renderer_fixtures::create_shapes()?)?;
    Ok(())
}
