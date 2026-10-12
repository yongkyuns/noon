//! Run with `cargo run -p noon-native --example foreground_matching`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::foreground_matching::program(
        true, 2.0,
    )?)?;
    Ok(())
}
