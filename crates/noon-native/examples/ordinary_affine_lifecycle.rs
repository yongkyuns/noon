//! Native presentation of the shared Rust/Python Grow/Spin/Shrink lifecycle example.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::ordinary_affine_lifecycle_program()?)?;
    Ok(())
}
