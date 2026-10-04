//! Run the same native builders used by paired browser qualification.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use noon::example_scenes::special_camera_settings::{self, CameraCase};
    let name = std::env::args().nth(1).unwrap_or_else(|| "illusion".into());
    let case = CameraCase::from_name(&name).ok_or("unknown camera case")?;
    if case.duration() == 0.0 {
        noon_native::run(special_camera_settings::static_session(case)?)?;
    } else {
        noon_native::run_live_program(special_camera_settings::program(case)?)?;
    }
    Ok(())
}
