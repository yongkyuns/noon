//! Native proof for compiler-ordered host callbacks on the canonical runtime.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (session, callbacks) = noon::example_scenes::live_affine_callbacks()?;
    noon_native::run_with_callbacks(session, callbacks)?;
    Ok(())
}
