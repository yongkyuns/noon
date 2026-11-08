//! Run with `cargo run -p noon-native --example ordinary_group_animation`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::group_animation::program()?)?;
    Ok(())
}
