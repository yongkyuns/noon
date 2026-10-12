//! Native host for the paired PolarPlane scene.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::polar_plane::session()?)?;
    Ok(())
}
