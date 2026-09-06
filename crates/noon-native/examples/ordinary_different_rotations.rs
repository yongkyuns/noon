//! Native presentation of the shared Rust/Python DifferentRotations example.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::ordinary_different_rotations_program()?)?;
    Ok(())
}
