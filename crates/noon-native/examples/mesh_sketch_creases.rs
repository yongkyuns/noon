//! Run the beveled cube crease demo in the native viewer.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::mesh_sketch_creases::program()?)?;
    Ok(())
}
