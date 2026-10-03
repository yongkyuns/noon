//! Native host proof for ordinary LTS matrix and regenerated Arrow targets.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::vector_space::session()?)?;
    Ok(())
}
