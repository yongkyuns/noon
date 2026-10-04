//! Native host proof for the public Line3D and explicit triangular Mesh3D profiles.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::spatial_primitives::session()?)?;
    Ok(())
}
