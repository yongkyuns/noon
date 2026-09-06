//! Run with `cargo run -p noon-native --example ordinary_mixed_scalar_composition`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::mixed_scalar_composition::program()?)?;
    Ok(())
}
