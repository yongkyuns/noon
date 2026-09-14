//! Run with `cargo run -p noon-native --example ordinary_cyclic_replace`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::cyclic_replace::program()?)?;
    Ok(())
}
