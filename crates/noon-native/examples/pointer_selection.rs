//! Run `cargo run -p noon-native --example pointer_selection`.
//! Click a filled shape to Indicate it; Rust restores its appearance automatically.
//! No authored timeline or host callback is needed; scene time stays at t=0.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session =
        noon::example_scenes::pointer_selection::click_indicate_scene()?.execution_session()?;
    noon_native::run(session)?;
    Ok(())
}
