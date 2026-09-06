//! Native host proof for canonical semantic native-input declarations.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session = noon::example_scenes::live_native_signals()?;
    noon_native::run(session)?;
    Ok(())
}
