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

    let mut session = scene.execution_session()?;
    let unaffected_slot = session.execution_slot_for_frame_index(1).unwrap();
    {
        let mut live = scene.live(&mut session);
        live.set_translation(&target, 2.0, -1.0)?;
        live.replace_content(&target, &replacement_text)?;
        assert_eq!(live.effective(&target)?.transform.translation.x, 2.0);
    }
    assert!(session.frame().objects[0].text().is_some());
    assert_eq!(
        session.execution_slot_for_frame_index(1),
        Some(unaffected_slot)
    );

    scene
        .live(&mut session)
        .replace_content(&target, &replacement_geometry)?;
    assert!(session.frame().objects[0].geometry().is_some());
    assert_eq!(
        session.execution_slot_for_frame_index(1),
        Some(unaffected_slot)
    );
    // Finish with both content kinds visible through the shared renderer. Content
    // replacement preserves presentation, so set the text's point-to-scene scale.
    {
        let mut live = scene.live(&mut session);
        live.replace_content(&target, &replacement_text)?;
        live.set_translation(&target, -2.0, 1.0)?;
        live.set_scale(&target, 1.0 / 96.0, 1.0 / 96.0)?;
    }
    noon_native::run(session)?;
    Ok(())
}
