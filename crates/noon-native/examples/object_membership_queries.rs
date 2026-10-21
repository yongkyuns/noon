//! The same typed scene is rendered by the direct Rust/WASM host.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::object_membership_queries::program()?)?;
    Ok(())
}
