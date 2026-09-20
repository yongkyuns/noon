//! Run `cargo run -p noon-native --example pointer_selection`.
//! Click a filled shape to tint it; click the background to clear selection.
//! There is no authored animation or input callback, so the scene stays at t=0.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut session = noon::example_scenes::pointer_selection::session()?;
    session.enable_pointer_fill_selection(4.0)?;
    noon_native::run(session)?;
    Ok(())
}
