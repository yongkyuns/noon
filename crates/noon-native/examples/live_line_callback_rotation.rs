//! Native proof for compiler-windowed Line rotation callbacks on the canonical runtime.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (session, callbacks) = noon::example_scenes::live_line_callback_rotation()?;
    noon_native::run_with_callbacks(session, callbacks)?;
    Ok(())
}
