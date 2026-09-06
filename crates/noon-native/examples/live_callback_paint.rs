//! Native proof for shared callback paint semantics on the canonical runtime.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (session, callbacks) = noon::example_scenes::live_callback_paint()?;
    noon_native::run_with_callbacks(session, callbacks)?;
    Ok(())
}
