//! Native host proof for the shared retained raster-image continuation.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::raster_image::program()?)?;
    Ok(())
}
