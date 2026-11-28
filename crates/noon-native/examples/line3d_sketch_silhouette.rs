//! Run the Line3D sketch silhouette in the native viewer.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run_live_program(noon::example_scenes::line3d_sketch_silhouette::program()?)?;
    Ok(())
}
