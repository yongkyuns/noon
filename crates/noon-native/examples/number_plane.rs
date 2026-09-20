//! Native host for the paired NumberPlane gallery scene.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::number_plane::session()?)?;
    Ok(())
}
