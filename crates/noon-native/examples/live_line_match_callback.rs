//! Native proof for shared analytic Line matching through ordered callbacks.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (session, callbacks) = noon::example_scenes::live_line_match_callback()?;
    noon_native::run_with_callbacks(session, callbacks)?;
    Ok(())
}
