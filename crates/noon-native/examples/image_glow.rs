//! Compare an ordinary image with the same image under animated glow.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::image_glow::program()?)?;
    Ok(())
}
