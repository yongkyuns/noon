//! Native renderer proof for persistent affine animation completion and replay.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session = noon::example_scenes::live_affine_completion()?;
    noon_native::run(session)?;
    Ok(())
}
