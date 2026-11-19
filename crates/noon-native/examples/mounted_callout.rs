//! Run with `cargo run -p noon-native --example mounted_callout`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::mounted_callout::program()?)?;
    Ok(())
}
