//! Run with `cargo run -p noon-native --example ordinary_affine_fade`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::affine_fade::program()?)?;
    Ok(())
}
