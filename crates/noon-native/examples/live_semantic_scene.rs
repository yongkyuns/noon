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
    let mut session = scene.into_execution()?;
    let anchor_slot = session.session().execution_slot_for_frame_index(0).unwrap();

    {
        session.scene_mut().remove(&toggled)?;
        assert!(session.scene().effective(&toggled).is_err());
        session.scene_mut().add(&toggled)?;
        session.scene_mut().add(&appended)?;
        session.scene_mut().set_translation(&appended, 2.0, -1.0)?;
        assert_eq!(
            session
                .scene()
                .effective(&appended)?
                .transform
                .translation
                .x,
            2.0
        );
    }

    assert_eq!(
        session.session().execution_slot_for_frame_index(0),
        Some(anchor_slot)
    );
    noon_native::run(session)?;
    Ok(())
}
