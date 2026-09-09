//! Identical geometry workload for every measured provider configuration.
use noon::{Scene, TextResource, Vec2};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0)?;
    scene.add(&circle)?;
    let mut session = scene.execution_session()?;
    session.take_frame_changes();
    {
        let mut live = scene.live(&mut session);
        live.set_translation(&circle, 2.0, -1.0)?;
        assert_eq!(
            live.effective(&circle)?.transform.translation,
            Vec2::new(2.0, -1.0)
        );
        assert_eq!(live.authored(&circle)?.transform.translation.x, 2.0);
    }
    assert_eq!(session.frame().objects.len(), 1);
    assert!(session.frame().objects[0].geometry().is_some());
    assert!(scene.store().borrow().text_resources().is_empty());
    // Shared text contracts remain available even when no provider is selected.
    let _: Option<TextResource> = None;
    std::hint::black_box(session.frame().objects.len());
    Ok(())
}
