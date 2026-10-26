//! Run `cargo run -p noon-native --example click_glow_pulse`.
//! Click the circle to pulse its persistent blue glow, or the square to create
//! a temporary glow. Both restore automatically while authored time stays at 0.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::click_glow_pulse::execution()?)?;
    Ok(())
}
