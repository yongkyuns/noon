"""Update old category assertions and prove a rejection preserves queued work."""
from pathlib import Path
path=Path('crates/noon/src/live_session.rs')
s=path.read_text()
for before,after in [
    ('live.target_editor(&circle),\n            Err(LiveSessionError::Mobject(_))', 'live.target_editor(&circle),\n            Err(LiveSessionError::Callback(crate::ExecutionSessionCallbackError::Pending(_)))'),
    ('live.move_to_point(&circle, 3.0, 0.0),\n            Err(LiveSessionError::Mobject(_))', 'live.move_to_point(&circle, 3.0, 0.0),\n            Err(LiveSessionError::Authoring(AuthoringError::Unsupported(\n                crate::UnsupportedAuthoringOperation::PlacementEffectiveAffineDriver\n            )))'),
]:
    assert s.count(before)==1,before
    s=s.replace(before,after)
path.write_text(s)

path=Path('../tooling/.github/r2a-tests/object_authoring_errors.rs')
s=path.read_text()
assert 'rejection_retains_previously_queued_changes_and_local_recovery' not in s
s+='''

#[test]
fn rejection_retains_previously_queued_changes_and_local_recovery() -> TestResult {
    let mut scene = Scene::new();
    let first = scene.circle(1.0)?;
    let second = scene.square(1.0)?;
    scene.add(&first)?;
    scene.add(&second)?;
    let mut session = scene.execution_session()?;
    session.take_frame_changes();
    scene.live(&mut session).shift(&first, 1.0, 0.0)?;
    let before = snapshot(&scene, &[&first, &second]);
    let frame = session.frame().clone();
    let publication = session.publication_context();
    assert!(matches!(
        scene.live(&mut session).set_fill_opacity(&second, -1.0),
        Err(LiveSessionError::Authoring(AuthoringError::InvalidOpacity { .. }))
    ));
    assert_eq!(snapshot(&scene, &[&first, &second]), before);
    assert_eq!(session.frame(), &frame);
    assert_eq!(session.publication_context(), publication);
    // The successful first edit must neither disappear nor be expanded to the
    // unrelated second row merely because the later operation was rejected.
    assert_eq!(session.take_frame_changes().object_indices(), &[0]);
    scene.live(&mut session).set_fill_opacity(&second, 0.5)?;
    assert_eq!(session.take_frame_changes().object_indices(), &[1]);
    Ok(())
}
'''
path.write_text(s)
