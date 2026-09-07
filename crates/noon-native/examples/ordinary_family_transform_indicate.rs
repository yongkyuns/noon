//! Run with `cargo run -p noon-native --example ordinary_family_transform_indicate`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::family_transform_indicate::program()?)?;
    Ok(())
}
