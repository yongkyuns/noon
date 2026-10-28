//! Move a glass card over a textured background using the shared live timeline.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::glass_lens::program()?)?;
    Ok(())
}
