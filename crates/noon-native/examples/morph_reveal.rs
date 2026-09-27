//! Direct Rust counterpart of the ordinary Python morph/reveal scene.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::renderer_fixtures::morph_reveal()?)?;
    Ok(())
}
