//! Shared exact-Line PassingFlash on the native host.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::line_passing_flash::program()?)?;
    Ok(())
}
