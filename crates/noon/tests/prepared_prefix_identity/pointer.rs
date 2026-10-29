//! Public reader ingress: no explicit target or direct animation start at click time.
use super::*;
use noon::integration::{
    NativePointerInputPublication, NativePointerInputToken, PointerFillOutcome, PointerFrameError,
    PointerFrameSnapshot, PointerFrameView,
};
use noon::{ExecutionSessionInputError, Scene};
use noon_core::{
    NativeInputModifiers, NativePointerId, NativePointerInput, NativePointerInputKind,
};

fn configure(reader: &mut PrefixReader, selection: bool) {
    if selection {
        reader.enable_pointer_fill_selection(4.0).unwrap();
    }
    reader
        .configure_native_pointer_input(
            NativePointerId {
                source: 71,
                pointer: 2,
            },
            1,
        )
        .unwrap();
}
fn displayed(reader: &PrefixReader) -> (PointerFrameSnapshot, PointerFrameView) {
    let view = reader
        .inspection_pointer_view(1, Vec2::new(200.0, 200.0))
        .unwrap();
    (reader.capture_pointer_frame(view).unwrap(), view)
}
fn event(
    token: &NativePointerInputToken,
    snapshot: &PointerFrameSnapshot,
    sequence: u64,
    press: bool,
) -> NativePointerInput {
    let position = snapshot.position(Vec2::new(100.0, 100.0)).unwrap();
    NativePointerInput::new(
        sequence,
        token.pointer(),
        token.context(),
        NativeInputModifiers::default(),
        if press {
            NativePointerInputKind::Press {
                position,
                button: 0,
            }
        } else {
            NativePointerInputKind::Release {
                position,
                button: 0,
            }
        },
    )
}
fn submit(reader: &mut PrefixReader, sequence: u64, press: bool) -> NativePointerInputPublication {
    let (snapshot, view) = displayed(reader);
    let token = reader
        .pointer_input_token_for_frame(&snapshot, view)
        .unwrap();
    reader
        .submit_native_pointer_input(&token, event(&token, &snapshot, sequence, press))
        .unwrap()
}
fn click(reader: &mut PrefixReader, sequence: u64) -> NativePointerInputPublication {
    submit(reader, sequence, true);
    submit(reader, sequence + 1, false)
}
fn finish_effect(
    reader: &mut PrefixReader,
    object: ObjectId,
    action: SemanticClickIndicate,
    wall: f64,
) {
    let baseline = reader.frame().clone();
    let history = reader.replay_stats();
    let context = reader.publication_context();
    assert!(reader.interactions_active());
    reader.advance_interactions(wall).unwrap();
    reader
        .advance_interactions(wall + action.run_time() / 2.0)
        .unwrap();
    let row = reader
        .frame()
        .objects
        .iter()
        .find(|row| row.id == object)
        .unwrap();
    assert_eq!(row.style.fill, Some(action.color()));
    assert_eq!(
        row.transform.scale,
        Vec2::new(action.scale_factor() as f32, action.scale_factor() as f32)
    );
    assert_eq!(reader.frame().time, baseline.time);
    reader
        .advance_interactions(wall + action.run_time())
        .unwrap();
    assert!(!reader.interactions_active());
    assert_eq!(reader.frame(), &baseline);
    assert_eq!(reader.replay_stats(), history);
    assert_eq!(
        reader.publication_context().scene_revision(),
        context.scene_revision()
    );
    assert_eq!(
        reader.publication_context().execution_revision(),
        context.execution_revision()
    );
}

#[test]
fn prefix_pointer_bound_click_works_without_selection_or_producer() {
    let mut source = Producer::new();
    let node = source.original;
    let object = source.object;
    let (_, mut reader) = source.bootstrap();
    assert_ne!(reader.runtime_identity(), source.session.runtime_identity());
    reader.seek(0.5).unwrap();
    assert!(reader.has_native_pointer_subscribers());
    configure(&mut reader, false);
    drop(source);
    let receipt = click(&mut reader, 0);
    assert_eq!(receipt.selection_click().unwrap().target(), Some(node));
    assert_eq!(reader.selected_pointer_target(), None);
    assert_eq!(receipt.selection_query().unwrap().precise_tests(), 1);
    assert_eq!(
        receipt
            .selection_query()
            .unwrap()
            .spatial_stats()
            .full_scan_fallbacks,
        0
    );
    finish_effect(&mut reader, object, A, 10.0);
}

#[test]
fn prefix_pointer_bootstrap_recovers_identity_deleted_before_reader_creation() {
    let mut source = Producer::new();
    let node = source.original;
    let object = source.object;
    let first = source.session.wait_segment(0.5).unwrap();
    source.session.advance_to(first.end_time()).unwrap();
    source.delete();
    let second = source.session.wait_segment(0.5).unwrap();
    source.session.advance_to(second.end_time()).unwrap();
    assert_eq!(source.session.execution_object_id(node), None);
    let (_, mut reader) = source.session.create_prepared_prefix(second).unwrap();
    drop(source);
    reader.seek(0.25).unwrap();
    configure(&mut reader, true);
    assert_eq!(
        click(&mut reader, 0).selection_click().unwrap().target(),
        Some(node)
    );
    finish_effect(&mut reader, object, A, 10.0);
    reader.advance_to(0.75).unwrap();
    assert_eq!(reader.selected_pointer_target(), None);
    assert_eq!(
        click(&mut reader, 2).selection_click().unwrap().target(),
        None
    );
}

#[test]
fn prefix_pointer_future_delivery_preserves_press_and_uses_displayed_generation() {
    let mut source = Producer::new();
    let old = source.original;
    let old_object = source.object;
    let (mut publisher, mut reader) = source.bootstrap();
    reader.seek(0.5).unwrap();
    configure(&mut reader, true);
    submit(&mut reader, 0, true);
    let (snapshot, view) = displayed(&reader);
    let token = reader
        .pointer_input_token_for_frame(&snapshot, view)
        .unwrap();
    let frame = reader.frame().clone();
    let identity = reader.runtime_identity();
    source.delete();
    append(&mut reader, source.publish(&mut publisher));
    let new_object = source.replacement();
    let new = noon_compile::semantic_execution_node_id(new_object);
    append(&mut reader, source.publish(&mut publisher));
    assert_eq!(reader.frame(), &frame);
    assert_eq!(reader.runtime_identity(), identity);
    assert_eq!(reader.publication_context(), snapshot.publication());
    assert_eq!(source.session.execution_object_id(old), None);
    drop(source);
    drop(publisher);
    let receipt = reader
        .submit_native_pointer_input(&token, event(&token, &snapshot, 1, false))
        .unwrap();
    assert_eq!(receipt.selection_click().unwrap().target(), Some(old));
    assert_eq!(reader.selected_pointer_target(), Some(old));
    assert_eq!(reader.pointer_selection_highlight().unwrap().target, old);
    finish_effect(&mut reader, old_object, A, 10.0);

    reader.seek(1.5).unwrap();
    assert_eq!(
        click(&mut reader, 2).selection_click().unwrap().target(),
        None
    );
    assert_eq!(reader.selected_pointer_target(), None);
    assert!(!reader.interactions_active());
    reader.seek(2.5).unwrap();
    assert_eq!(
        click(&mut reader, 4).selection_click().unwrap().target(),
        Some(new)
    );
    finish_effect(&mut reader, new_object, B, 20.0);
    reader.seek(0.5).unwrap();
    assert_eq!(
        click(&mut reader, 6).selection_click().unwrap().target(),
        Some(old)
    );
    finish_effect(&mut reader, old_object, A, 30.0);
}

#[test]
fn prefix_pointer_unavailable_seek_and_stale_input_are_atomic() {
    let mut source = Producer::new();
    let (_, mut reader) = source.bootstrap();
    reader.seek(0.5).unwrap();
    configure(&mut reader, true);
    submit(&mut reader, 0, true);
    let (snapshot, view) = displayed(&reader);
    let token = reader
        .pointer_input_token_for_frame(&snapshot, view)
        .unwrap();
    let frame = reader.frame().clone();
    for time in [reader.frontier(), -1.0, f64::NAN, f64::INFINITY] {
        assert!(reader.seek(time).is_err());
        assert_eq!(reader.frame(), &frame);
        assert_eq!(reader.publication_context(), snapshot.publication());
    }
    let release = reader
        .submit_native_pointer_input(&token, event(&token, &snapshot, 1, false))
        .unwrap();
    assert_eq!(
        release.selection_click().unwrap().target(),
        Some(source.original)
    );
    assert!(reader.interactions_active());
    reader.seek(0.5).unwrap();
    assert!(!reader.interactions_active());
    assert!(reader.selected_pointer_target().is_none());
    assert!(matches!(
        reader.pointer_input_token_for_frame(&snapshot, view),
        Err(PointerFrameError::Input(
            ExecutionSessionInputError::StalePointerPublication { .. }
        ))
    ));
    assert!(matches!(
        reader.submit_native_pointer_input(&token, event(&token, &snapshot, 2, true)),
        Err(ExecutionSessionInputError::StalePointerPublication { .. })
    ));
    // Rejection did not consume sequence 2. A seek cancelled, not completed, the gesture.
    assert!(submit(&mut reader, 2, false).selection_click().is_none());
    assert_eq!(
        click(&mut reader, 3).selection_click().unwrap().target(),
        Some(source.original)
    );
}

#[test]
fn prefix_pointer_foreign_reader_cannot_reuse_presented_input() {
    let mut source = Producer::new();
    let boundary = source.session.wait_segment(1.0).unwrap();
    source.session.advance_to(1.0).unwrap();
    let (_, mut left) = source.session.create_prepared_prefix(boundary).unwrap();
    let (_, mut right) = source.session.create_prepared_prefix(boundary).unwrap();
    left.seek(0.5).unwrap();
    right.seek(0.5).unwrap();
    configure(&mut left, true);
    configure(&mut right, true);
    let (snapshot, view) = displayed(&left);
    let token = left.pointer_input_token_for_frame(&snapshot, view).unwrap();
    let query = left.query_viewport(noon_core::Rect::new(
        Vec2::new(-2.0, -2.0),
        Vec2::new(2.0, 2.0),
    ));
    assert!(left.viewport_query_is_current(&query));
    assert!(!right.viewport_query_is_current(&query));
    let frame = right.frame().clone();
    let context = right.publication_context();
    assert!(matches!(
        right.pointer_input_token_for_frame(&snapshot, view),
        Err(PointerFrameError::Input(
            ExecutionSessionInputError::ForeignPointerRuntime
        ))
    ));
    assert!(matches!(
        right.submit_native_pointer_input(&token, event(&token, &snapshot, 0, true)),
        Err(ExecutionSessionInputError::ForeignPointerRuntime)
    ));
    assert_eq!(right.frame(), &frame);
    assert_eq!(right.publication_context(), context);
    assert_eq!(
        click(&mut right, 0).selection_click().unwrap().target(),
        Some(source.original)
    );
    assert!(!left.interactions_active());
}

#[test]
fn prefix_pointer_stationary_hover_follows_selected_history_not_received_rows() {
    let mut source = Producer::new();
    let old = source.original;
    let (mut publisher, mut reader) = source.bootstrap();
    reader.seek(0.5).unwrap();
    configure(&mut reader, true);
    let (snapshot, view) = displayed(&reader);
    let token = reader
        .pointer_input_token_for_frame(&snapshot, view)
        .unwrap();
    let motion = NativePointerInput::new(
        0,
        token.pointer(),
        token.context(),
        NativeInputModifiers::default(),
        NativePointerInputKind::Move(snapshot.position(Vec2::new(100.0, 100.0)).unwrap()),
    );
    reader.submit_native_pointer_input(&token, motion).unwrap();
    let (snapshot, view) = displayed(&reader);
    let enter = reader
        .refresh_pointer_hover(&snapshot, view)
        .unwrap()
        .unwrap();
    assert_eq!((enter.previous, enter.current), (None, Some(old)));
    source.delete();
    append(&mut reader, source.publish(&mut publisher));
    let new = noon_compile::semantic_execution_node_id(source.replacement());
    append(&mut reader, source.publish(&mut publisher));
    assert!(reader
        .refresh_pointer_hover(&snapshot, view)
        .unwrap()
        .is_none());
    assert_eq!(reader.hovered_pointer_target(), Some(old));
    reader.advance_to(1.5).unwrap();
    assert!(reader.refresh_pointer_hover(&snapshot, view).is_err());
    let (absent, view) = displayed(&reader);
    let leave = reader
        .refresh_pointer_hover(&absent, view)
        .unwrap()
        .unwrap();
    assert_eq!((leave.previous, leave.current), (Some(old), None));
    reader.advance_to(2.5).unwrap();
    let (present, view) = displayed(&reader);
    let enter = reader
        .refresh_pointer_hover(&present, view)
        .unwrap()
        .unwrap();
    assert_eq!((enter.previous, enter.current), (None, Some(new)));
    assert_eq!(enter.precise_tests, 1);
    assert_eq!(enter.spatial_stats.full_scan_fallbacks, 0);
    assert!(reader
        .refresh_pointer_hover(&present, view)
        .unwrap()
        .is_none());
    reader.seek(0.5).unwrap();
    assert_eq!(reader.hovered_pointer_target(), None);
}

#[test]
fn prefix_pointer_inspection_zoom_preserves_effect_and_survives_seek() {
    let mut source = Producer::new();
    let (_, mut reader) = source.bootstrap();
    reader.seek(0.5).unwrap();
    configure(&mut reader, true);
    click(&mut reader, 0);
    reader.advance_interactions(10.0).unwrap();
    reader.advance_interactions(10.125).unwrap();
    let frame = reader.frame().clone();
    let context = reader.publication_context();
    let before = reader.inspection_camera().unwrap();
    let (snapshot, view) = displayed(&reader);
    assert!(reader
        .zoom_inspection_view(&snapshot, view, Vec2::new(100.0, 100.0), 0.5)
        .unwrap());
    assert_eq!(reader.frame(), &frame);
    assert_eq!(reader.publication_context(), context);
    assert!(reader.interactions_active());
    let zoomed = reader.inspection_camera().unwrap();
    assert_ne!(zoomed, before);
    assert!(reader
        .pointer_input_token_for_frame(&snapshot, view)
        .is_err());
    reader.advance_interactions(10.5).unwrap();
    reader.seek(0.25).unwrap();
    assert_eq!(reader.inspection_camera().unwrap(), zoomed);
    assert!(!reader.interactions_active());
    configure(&mut reader, true);
    let receipt = click(&mut reader, 2);
    assert_eq!(
        receipt.selection_query().unwrap().outcome(),
        PointerFillOutcome::Hit(source.original)
    );
}

#[test]
fn prefix_pointer_selection_does_not_revive_after_observed_detach_and_reentry() {
    let mut scene = Scene::new();
    let mut shape = scene.circle(0.65).unwrap();
    shape.set_fill(0.1, 0.3, 0.9, 1.0).unwrap();
    scene.add(&shape).unwrap();
    let mut source = scene.execution_session().unwrap();
    source
        .begin_replay_retention(ReplayLimits {
            revisions: 64,
            payloads: 256,
        })
        .unwrap();
    let first = source.wait_segment(1.0).unwrap();
    source.advance_to(1.0).unwrap();
    let (mut publisher, mut reader) = source.create_prepared_prefix(first).unwrap();
    reader.seek(0.5).unwrap();
    configure(&mut reader, true);
    assert_eq!(
        click(&mut reader, 0).selection_click().unwrap().target(),
        Some(shape.node_id())
    );
    scene.live(&mut source).remove(&shape).unwrap();
    let second = source.wait_segment(1.0).unwrap();
    source.advance_to(2.0).unwrap();
    append(
        &mut reader,
        source
            .publish_prepared_prefix(second, &mut publisher)
            .unwrap(),
    );
    scene.live(&mut source).add(&shape).unwrap();
    let third = source.wait_segment(1.0).unwrap();
    source.advance_to(3.0).unwrap();
    append(
        &mut reader,
        source
            .publish_prepared_prefix(third, &mut publisher)
            .unwrap(),
    );
    reader.advance_to(1.5).unwrap();
    assert_eq!(reader.selected_pointer_target(), None);
    reader.advance_to(2.5).unwrap();
    assert_eq!(reader.selected_pointer_target(), None);
    assert_eq!(
        click(&mut reader, 2).selection_click().unwrap().target(),
        Some(shape.node_id())
    );
}
