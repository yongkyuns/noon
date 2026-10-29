//! The ordinary HDR material/view example also runs through direct Rust/WASM.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::camera_bloom::program()?)?;
    Ok(())
}
