//! Identical geometry workload for every measured provider configuration.
use noon::{Scene, Vec2};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0)?;
    scene.add(&circle)?;
    let mut session = scene.into_execution()?;
    session.session_mut().take_frame_changes();
    {
        session.scene_mut().set_translation(&circle, 2.0, -1.0)?;
        assert_eq!(
            session.scene().effective(&circle)?.transform.translation,
            Vec2::new(2.0, -1.0)
        );
        assert_eq!(circle.state()?.transform.translation.x, 2.0);
    }
    assert_eq!(session.session().frame().objects.len(), 1);
    assert!(session.session().frame().objects[0].geometry().is_some());
    std::hint::black_box(session.session().frame().objects.len());
    Ok(())
}
