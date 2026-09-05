//! Native renderer proof for the canonical scalar `ValueTracker` timeline.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session = noon::example_scenes::live_value_tracker()?;
    noon_native::run(session)?;
    Ok(())
}
