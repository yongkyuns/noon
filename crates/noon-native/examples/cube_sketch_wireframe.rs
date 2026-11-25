//! Run the cube sketch wireframe in the native viewer.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::cube_sketch_wireframe::program()?)?;
    Ok(())
}
