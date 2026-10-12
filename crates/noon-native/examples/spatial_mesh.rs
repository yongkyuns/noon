//! Native host proof for the shared camera and retained mesh scene.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::spatial_mesh::session()?)?;
    Ok(())
}
