//! Paired Rust proof for live property and structural membership publication.
use noon::Scene;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let anchor = scene.circle(0.5)?;
    let toggled = scene.circle(1.0)?;
    // Author detached objects before runtime bootstrap so the session starts at the
    // same semantic revision and later publishes only their membership.
    let appended = scene.square(1.5)?;
    scene.add(&anchor)?;
    scene.add(&toggled)?;
    let mut session = scene.execution_session()?;
    let anchor_slot = session.execution_slot_for_frame_index(0).unwrap();

    {
        let mut live = scene.live(&mut session);
        live.remove(&toggled)?;
        assert!(live.effective(&toggled).is_err());
        live.add(&toggled)?;
        live.add(&appended)?;
        live.set_translation(&appended, 2.0, -1.0)?;
        assert_eq!(live.effective(&appended)?.transform.translation.x, 2.0);
    }

    assert_eq!(session.execution_slot_for_frame_index(0), Some(anchor_slot));
    noon_native::run(session)?;
    Ok(())
}
