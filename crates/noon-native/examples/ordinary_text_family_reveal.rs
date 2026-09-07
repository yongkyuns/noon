//! Run with `cargo run -p noon-native --example ordinary_text_family_reveal`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::text_family_reveal::program()?)?;
    Ok(())
}
