//! Native host for the shared mixed spatial text/composition fixture.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::spatial_camera_labels::session()?)?;
    Ok(())
}
