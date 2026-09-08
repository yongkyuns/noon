//! Run with `cargo run --example ordinary_family_placement`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::family_placement::session()?)?;
    Ok(())
}
