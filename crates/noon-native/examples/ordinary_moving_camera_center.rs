//! Native execution of the target-neutral MovingCameraCenter program.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::ordinary_moving_camera_center_program()?)?;
    Ok(())
}
