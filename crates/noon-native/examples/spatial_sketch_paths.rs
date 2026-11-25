//! Run the seekable 3D sketch path demo with the native Rust viewer.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::spatial_sketch_paths::program()?)?;
    Ok(())
}
