//! Run `cargo run -p noon-native --example pointer_selection`.
//! Click a filled shape to tint it; click the background to clear selection.
//! There is no authored animation or input callback, so the scene stays at t=0.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = noon::Scene::new();
    let mut circle = scene.circle(1.0)?;
    circle.set_fill(0.1, 0.3, 0.9, 1.0)?;
    circle.set_translation(-1.5, 0.0)?;
    scene.add(&circle)?;
    let mut rectangle = scene.square(1.6)?;
    rectangle.set_fill(0.1, 0.7, 0.3, 1.0)?;
    rectangle.set_translation(1.5, 0.0)?;
    rectangle.set_rotation(0.4)?;
    scene.add(&rectangle)?;
    let mut session = scene.execution_session()?;
    session.enable_pointer_fill_selection(4.0)?;
    noon_native::run(session)?;
    Ok(())
}
