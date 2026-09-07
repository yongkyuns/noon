//! Run with `cargo run -p noon-native --example ordinary_text_write`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::text_write::program()?)?;
    Ok(())
}
