//! Native host for the paired probability partition scene.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::sample_space::session()?)?;
    Ok(())
}
