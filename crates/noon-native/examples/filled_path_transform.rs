//! Direct native Rust example, paired with the ordinary Python filled_path_transform scene.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::renderer_fixtures::filled_path_transform()?)?;
    Ok(())
}
