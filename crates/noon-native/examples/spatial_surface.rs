//! Native host proof for the shared UV surface and world-rotation intent.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::spatial_surface::session()?)?;
    Ok(())
}
