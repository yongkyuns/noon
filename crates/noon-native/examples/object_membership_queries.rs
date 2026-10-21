//! The same typed scene is rendered by the direct Rust/WASM host.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::object_membership_queries::session()?)?;
    Ok(())
}
