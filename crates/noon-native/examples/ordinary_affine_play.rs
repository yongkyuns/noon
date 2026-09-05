//! Native renderer proof for sequential ordinary affine plays on one runtime.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session = noon::example_scenes::ordinary_affine_play()?;
    noon_native::run(session)?;
    Ok(())
}
