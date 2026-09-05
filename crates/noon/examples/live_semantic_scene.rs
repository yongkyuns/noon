//! Paired Rust proof for the bounded typed live property/query path.
use noon::Scene;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0)?;
    scene.add(&circle)?;
    let mut session = scene.execution_session()?;

    let mut live = scene.live(&mut session);
    live.set_translation(&circle, 2.0, -1.0)?;
    live.set_scale(&circle, 1.5, 0.5)?;
    let effective = live.effective(&circle)?;
    assert_eq!(effective.transform.translation.x, 2.0);
    assert_eq!(effective.transform.translation.y, -1.0);
    assert_eq!(effective.transform.scale.x, 1.5);
    Ok(())
}
