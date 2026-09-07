//! Run with `cargo run --example specialized_geometry`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::specialized_geometry::session()?)?;
    Ok(())
}
