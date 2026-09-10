//! Atomic family fill/stroke edits through the ordinary native host.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::family_paint::session()?)?;
    Ok(())
}
