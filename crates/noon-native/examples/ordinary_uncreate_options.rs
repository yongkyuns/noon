//! Run with `cargo run -p noon-native --example ordinary_uncreate_options`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::ordinary_uncreate_options::program()?)?;
    Ok(())
}
