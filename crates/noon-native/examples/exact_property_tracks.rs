//! Run with `cargo run -p noon-native --example exact_property_tracks`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::exact_property_tracks::session()?)?;
    Ok(())
}
