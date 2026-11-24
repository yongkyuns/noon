//! A thousand ordinary Glow attachment/removal cycles among 600 static dots.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::glow_churn::program()?)?;
    Ok(())
}
