//! Run with `cargo run -p noon-native --example continuous_flow`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::continuous_flow::session()?)?;
    Ok(())
}
