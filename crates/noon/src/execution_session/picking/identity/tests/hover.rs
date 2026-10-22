use super::*;
use crate::execution_session::{PointerFrameError, PointerFrameSnapshot, PointerFrameView};

fn observe(
    session: &mut ExecutionSession,
    view: PointerFrameView,
    previous: Option<SemanticNodeId>,
    current: Option<SemanticNodeId>,
) -> PointerFrameSnapshot {
    let displayed = session.capture_pointer_frame(view).unwrap();
    session.take_frame_changes();
    let baseline = session.frame().clone();
    let publication = session.publication_context();
    let history = session.replay_stats();
    let transition = session
        .refresh_pointer_hover(&displayed, view)
        .unwrap()
        .unwrap();
    assert_eq!((transition.previous, transition.current), (previous, current));
    assert_eq!(transition.publication, publication);
    assert_eq!(transition.spatial_stats.full_scan_fallbacks, 0);
    assert_eq!(transition.precise_tests, usize::from(current.is_some()));
    assert_eq!(session.hovered_pointer_target(), current);
    // The same clean publication is not picked or published again.
    assert!(session
        .refresh_pointer_hover(&displayed, view)
        .unwrap()
        .is_none());
    assert_eq!(session.frame(), &baseline);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.replay_stats(), history);
    assert!(session.take_frame_changes().is_empty());
    assert_eq!(session.selected_pointer_target(), None);
    assert!(!session.interactions_active());
    displayed
}

#[test]
fn historical_pointer_identity_stationary_hover_tracks_deletion_and_reused_slot() {
    let mut fixture = Fixture::new();
    fixture.wait();
    fixture.delete();
    fixture.wait();
    let (replacement, _) = fixture.replacement();
    fixture.wait();
    let node = fixture.node;
    let mut session = fixture.seal();
    session.seek(0.5).unwrap();
    let camera = session.inspection_camera().unwrap();
    let view = PointerFrameView::new(1, Vec2::new(200.0, 200.0), camera).unwrap();
    let displayed = session.capture_pointer_frame(view).unwrap();
    let token = displayed.input_token(&session, view).unwrap();
    let motion = NativePointerInput::new(
        0,
        token.pointer(),
        token.context(),
        NativeInputModifiers::default(),
        NativePointerInputKind::Move(displayed.position(Vec2::new(100.0, 100.0)).unwrap()),
    );
    session.submit_native_pointer_input(&token, motion).unwrap();
    // Capture after admission, as a host does for the publication it presents.
    // This tests the shared observation contract, not GPU presentation itself.
    let old = observe(&mut session, view, None, Some(node));
    assert_eq!(session.execution_object_id(node), None);

    // No further pointer event: membership moves beneath the stationary cursor.
    session.advance_to(1.5).unwrap();
    assert_eq!(session.hovered_pointer_target(), None);
    let baseline = session.frame().clone();
    let publication = session.publication_context();
    let history = session.replay_stats();
    assert!(matches!(
        session.refresh_pointer_hover(&old, view),
        Err(PointerFrameError::Input(
            ExecutionSessionInputError::StalePointerPublication { .. }
        ))
    ));
    assert_eq!(session.frame(), &baseline);
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.replay_stats(), history);
    // A rejected old snapshot must not consume the pending leave transition.
    observe(&mut session, view, Some(node), None);

    session.advance_to(2.5).unwrap();
    assert_eq!(session.hovered_pointer_target(), None);
    observe(&mut session, view, None, Some(replacement));
    assert_eq!(session.execution_object_id(node), None);
    assert_eq!(node.slot(), replacement.slot());
    assert_ne!(node.generation(), replacement.generation());

    session.seek(0.5).unwrap();
    assert_eq!(session.hovered_pointer_target(), None);
    let displayed = session.capture_pointer_frame(view).unwrap();
    assert!(session
        .refresh_pointer_hover(&displayed, view)
        .unwrap()
        .is_none());
}
