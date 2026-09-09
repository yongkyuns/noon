//! Shared grid semantics through the ordinary native platform host.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::family_grid::session()?)?;
    Ok(())
}
