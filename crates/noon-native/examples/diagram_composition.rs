//! Run with `cargo run -p noon-native --example diagram_composition`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::diagram_composition::session()?)?;
    Ok(())
}
