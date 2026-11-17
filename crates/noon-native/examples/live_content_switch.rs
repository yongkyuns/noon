//! Shared live content replacement with resources authored before runtime bootstrap.
use noon::{Scene, Text};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut target = scene.circle(0.75)?;
    target.set_fill(1.0, 1.0, 1.0, 1.0)?;
    let mut unaffected = scene.square(1.0)?;
    unaffected.set_color(0.0, 0.0, 1.0, 1.0)?;
    let replacement_text = scene.text(Text::new("one runtime"))?;
    let replacement_geometry = scene.circle(1.5)?;
    scene.add(&target)?;
    scene.add(&unaffected)?;

    let mut session = scene.into_execution()?;
    let unaffected_slot = session.session().execution_slot_for_frame_index(1).unwrap();
    {
        session.scene_mut().set_translation(&target, 2.0, -1.0)?;
        session
            .scene_mut()
            .replace_content(&target, &replacement_text)?;
        assert_eq!(
            session.scene().effective(&target)?.transform.translation.x,
            2.0
        );
    }
    assert!(session.scene().effective_content(&target)?.text().is_some());
    assert_eq!(
        session.session().execution_slot_for_frame_index(1),
        Some(unaffected_slot)
    );

    session
        .scene_mut()
        .replace_content(&target, &replacement_geometry)?;
    assert!(session
        .scene()
        .effective_content(&target)?
        .geometry()
        .is_some());
    assert_eq!(
        session.session().execution_slot_for_frame_index(1),
        Some(unaffected_slot)
    );
    // Finish with both content kinds visible through the shared renderer. Content
    // replacement preserves presentation, so set the text's point-to-scene scale.
    {
        session
            .scene_mut()
            .replace_content(&target, &replacement_text)?;
        session.scene_mut().set_translation(&target, -2.0, 1.0)?;
        session
            .scene_mut()
            .set_scale(&target, 1.0 / 96.0, 1.0 / 96.0)?;
    }
    noon_native::run(session)?;
    Ok(())
}
