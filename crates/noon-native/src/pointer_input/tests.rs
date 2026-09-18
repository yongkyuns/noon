use super::*;
use crate::{execution_source::LiveProgramExecutionSource, NativeViewportConfig};
use noon::ExecutionSession;
use noon_core::{
    NativeEventOccurrence, NativeEventSource, ReactiveValue, SemanticMutationTransaction,
    SemanticNativeInputSource, SemanticNodeCreation, SemanticNodeId, SemanticObjectProperty,
    SemanticObjectState, SemanticSignalValue, SemanticStore, SemanticVec3, StoredGeometry,
};

const SIZE: PhysicalSize<u32> = PhysicalSize::new(800, 400);

struct Fixture {
    app: NativeApp,
    position: SemanticNodeId,
    button: SemanticNodeId,
    down: SemanticNodeId,
    up: SemanticNodeId,
    viewport: SemanticNodeId,
    unrelated: SemanticNodeId,
}

fn signal(
    store: &mut SemanticStore,
    root: SemanticNodeId,
    source: SemanticNativeInputSource,
    initial: SemanticSignalValue,
) -> SemanticNodeId {
    let mut tx = SemanticMutationTransaction::new();
    let pending =
        tx.create_node(SemanticNodeCreation::native_input_signal(initial, source).unwrap());
    tx.scope_signal(root, pending);
    tx.apply(store).unwrap().resolve(pending).unwrap()
}

impl Fixture {
    fn new() -> Self {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let target =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 0.5,
            }));
        let unrelated =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 0.5,
            }));
        for object in [target, unrelated] {
            store.add_semantic_family_member(root, object).unwrap();
        }
        let position = signal(
            &mut store,
            root,
            SemanticNativeInputSource::State(NativeStateSource::PointerPosition),
            SemanticSignalValue::Vec3(SemanticVec3::ZERO),
        );
        let button = signal(
            &mut store,
            root,
            SemanticNativeInputSource::State(NativeStateSource::PointerButton { button: 0 }),
            SemanticSignalValue::Bool(false),
        );
        let down = signal(
            &mut store,
            root,
            SemanticNativeInputSource::Event(NativeEventSource::PointerDown { button: 0 }),
            SemanticSignalValue::Scalar(0.0),
        );
        let up = signal(
            &mut store,
            root,
            SemanticNativeInputSource::Event(NativeEventSource::PointerUp { button: 0 }),
            SemanticSignalValue::Scalar(0.0),
        );
        let viewport = signal(
            &mut store,
            root,
            SemanticNativeInputSource::State(NativeStateSource::ViewportSize),
            SemanticSignalValue::Vec3(SemanticVec3::ZERO),
        );
        store
            .bind_semantic_signal(position, target, SemanticObjectProperty::Translation)
            .unwrap();
        store
            .bind_semantic_signal(down, target, SemanticObjectProperty::RotationZ)
            .unwrap();
        let session = ExecutionSession::from_semantic_root(&store, root).unwrap();
        Self {
            app: NativeApp::new(session, NativeViewportConfig::default()),
            position,
            button,
            down,
            up,
            viewport,
            unrelated,
        }
    }
    fn value(&self, signal: SemanticNodeId) -> &ReactiveValue {
        self.app.session().effective_signal_value(signal).unwrap()
    }
    fn move_to(&mut self, x: f64, y: f64) {
        self.app
            .dispatch_pointer_position(PhysicalPosition::new(x, y), SIZE, 1.0)
            .unwrap();
    }
    fn edge(&mut self, state: ElementState) {
        self.app
            .dispatch_pointer_button(MouseButton::Left, state, SIZE, 1.0)
            .unwrap();
    }
}

#[test]
fn paused_move_press_and_release_use_one_coherent_session_path() {
    let mut f = Fixture::new();
    f.move_to(200.0, 100.0);
    f.app.static_session_mut().take_frame_changes();
    let before = f.app.session().publication_context();
    f.edge(ElementState::Pressed);
    assert_eq!(
        f.value(f.position),
        &ReactiveValue::Vec2(Vec2::new(-4.0, 2.0))
    );
    assert_eq!(f.value(f.button), &ReactiveValue::Bool(true));
    assert_eq!(f.value(f.down), &ReactiveValue::Scalar(1.0));
    assert_eq!(f.app.session().frame().time, 0.0);
    assert_eq!(
        f.app.session().publication_context().frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
    assert_eq!(
        f.app
            .static_session_mut()
            .take_frame_changes()
            .object_indices(),
        &[0]
    );
    assert!(f.app.session().wake_state().is_quiescent());
    f.move_to(600.0, 300.0);
    f.edge(ElementState::Released);
    assert_eq!(
        f.value(f.position),
        &ReactiveValue::Vec2(Vec2::new(4.0, -2.0))
    );
    assert_eq!(f.value(f.button), &ReactiveValue::Bool(false));
    assert_eq!(f.value(f.up), &ReactiveValue::Scalar(1.0));
    assert_eq!(f.app.next_input_sequence, 4);
}

#[test]
fn repeated_edges_and_nonpointer_events_share_sequence_without_coalescing() {
    let mut f = Fixture::new();
    f.move_to(400.0, 200.0);
    f.edge(ElementState::Pressed);
    f.app.dispatch_event(NativeEventSource::Wheel).unwrap();
    f.edge(ElementState::Pressed);
    f.edge(ElementState::Released);
    f.edge(ElementState::Released);
    assert_eq!(f.value(f.down), &ReactiveValue::Scalar(2.0));
    assert_eq!(f.value(f.up), &ReactiveValue::Scalar(2.0));
    assert_eq!(f.app.next_input_sequence, 6);
    assert!(f
        .app
        .execution
        .emit_native_event(NativeEventOccurrence::new(5, NativeEventSource::Wheel))
        .is_err());
}

#[test]
fn first_button_without_position_cancels_instead_of_fabricating_an_edge() {
    let mut f = Fixture::new();
    f.edge(ElementState::Pressed);
    f.edge(ElementState::Released);
    assert_eq!(f.value(f.button), &ReactiveValue::Bool(false));
    assert_eq!(f.value(f.down), &ReactiveValue::Scalar(0.0));
    assert_eq!(f.value(f.up), &ReactiveValue::Scalar(0.0));
    assert_eq!(f.value(f.position), &ReactiveValue::Vec2(Vec2::ZERO));
    assert_eq!(f.app.next_input_sequence, 2);
    assert!(f.app.pointer.surface.is_none());
}

#[test]
fn focus_loss_clears_buttons_and_modifiers_but_does_not_release() {
    let mut f = Fixture::new();
    f.move_to(200.0, 100.0);
    f.app.pointer.modifiers = ModifiersState::SHIFT | ModifiersState::CONTROL;
    f.edge(ElementState::Pressed);
    f.app.pointer_focus_lost().unwrap();
    assert_eq!(f.value(f.button), &ReactiveValue::Bool(false));
    assert_eq!(f.value(f.up), &ReactiveValue::Scalar(0.0));
    assert!(f.app.pointer.surface.is_none());
    assert!(f.app.pointer.modifiers.is_empty());
    f.edge(ElementState::Released);
    assert_eq!(f.value(f.up), &ReactiveValue::Scalar(0.0));
    assert_eq!(
        f.value(f.position),
        &ReactiveValue::Vec2(Vec2::new(-4.0, 2.0))
    );
}

#[test]
fn leaving_or_zero_surface_cancels_and_requires_a_fresh_sample() {
    let mut f = Fixture::new();
    f.move_to(200.0, 100.0);
    f.edge(ElementState::Pressed);
    f.app.pointer_left().unwrap();
    assert_eq!(f.value(f.button), &ReactiveValue::Bool(false));
    f.edge(ElementState::Released);
    assert_eq!(f.value(f.up), &ReactiveValue::Scalar(0.0));
    f.move_to(600.0, 300.0);
    f.edge(ElementState::Pressed);
    f.app
        .dispatch_pointer_position(
            PhysicalPosition::new(1.0, 1.0),
            PhysicalSize::new(0, 0),
            1.0,
        )
        .unwrap();
    assert_eq!(f.value(f.button), &ReactiveValue::Bool(false));
    assert_eq!(
        f.value(f.position),
        &ReactiveValue::Vec2(Vec2::new(4.0, -2.0))
    );
    assert!(f.app.pointer.surface.is_none());
}

#[test]
fn hidpi_changes_logical_units_not_scene_position_and_invalidates_old_binding() {
    let mut f = Fixture::new();
    f.move_to(200.0, 100.0);
    f.edge(ElementState::Pressed);
    let old = f.app.execution.native_pointer_input_token().unwrap();
    f.app.pointer_scale_changed(SIZE, 2.0).unwrap();
    assert_eq!(
        f.value(f.viewport),
        &ReactiveValue::Vec2(Vec2::new(400.0, 200.0))
    );
    assert_eq!(f.value(f.button), &ReactiveValue::Bool(false));
    assert!(f.app.pointer.surface.is_none());
    let stale = NativePointerInput::new(
        f.app.next_input_sequence,
        old.pointer(),
        old.context(),
        NativeInputModifiers::default(),
        NativePointerInputKind::Cancel(NativePointerCancellation::FocusLost),
    );
    assert!(f
        .app
        .execution
        .submit_native_pointer_input(&old, stale)
        .is_err());
    f.app
        .dispatch_pointer_position(PhysicalPosition::new(200.0, 100.0), SIZE, 2.0)
        .unwrap();
    assert_eq!(
        f.value(f.position),
        &ReactiveValue::Vec2(Vec2::new(-4.0, 2.0))
    );
    assert_eq!(f.app.pointer.surface, Some(Vec2::new(100.0, 50.0)));
    assert_eq!(
        f.app
            .execution
            .native_pointer_input_token()
            .unwrap()
            .context()
            .view_revision,
        old.context().view_revision + 1
    );
}

#[test]
fn resize_rebinding_does_not_reuse_an_old_cursor_for_release() {
    let mut f = Fixture::new();
    f.move_to(200.0, 100.0);
    f.edge(ElementState::Pressed);
    f.app.rebind_pointer_view().unwrap();
    f.app
        .dispatch_pointer_button(
            MouseButton::Left,
            ElementState::Released,
            PhysicalSize::new(1200, 600),
            1.0,
        )
        .unwrap();
    assert_eq!(f.value(f.button), &ReactiveValue::Bool(false));
    assert_eq!(f.value(f.up), &ReactiveValue::Scalar(0.0));
    assert_eq!(
        f.value(f.position),
        &ReactiveValue::Vec2(Vec2::new(-4.0, 2.0))
    );
}

#[test]
fn nonfinite_or_unrepresentable_input_leaves_publication_sequence_and_cache_unchanged() {
    let mut f = Fixture::new();
    assert!(f
        .app
        .dispatch_pointer_position(PhysicalPosition::new(f64::NAN, 0.0), SIZE, 1.0)
        .is_err());
    assert!(!f.app.pointer.configured);
    f.move_to(200.0, 100.0);
    let publication = f.app.session().publication_context();
    let sequence = f.app.next_input_sequence;
    let surface = f.app.pointer.surface;
    for x in [f64::NAN, f64::INFINITY, f64::MAX] {
        assert!(f
            .app
            .dispatch_pointer_position(PhysicalPosition::new(x, 100.0), SIZE, 1.0)
            .is_err());
    }
    for scale in [
        0.0,
        -1.0,
        f64::NAN,
        f64::INFINITY,
        f64::MIN_POSITIVE,
        f64::MAX,
    ] {
        assert!(f.app.pointer_scale_changed(SIZE, scale).is_err());
    }
    assert_eq!(f.app.session().publication_context(), publication);
    assert_eq!(f.app.next_input_sequence, sequence);
    assert_eq!(f.app.pointer.surface, surface);
}

#[test]
fn pending_callback_rejects_bursts_without_acknowledging_or_overwriting_position() {
    let mut f = Fixture::new();
    f.move_to(200.0, 100.0);
    let overlay = f
        .app
        .static_session_mut()
        .begin_required_callback_phase(0.0, [f.unrelated])
        .unwrap();
    let publication = f.app.session().publication_context();
    let sequence = f.app.next_input_sequence;
    for _ in 0..128 {
        assert!(f
            .app
            .dispatch_pointer_position(PhysicalPosition::new(600.0, 300.0), SIZE, 1.0)
            .is_err());
    }
    assert_eq!(f.app.next_input_sequence, sequence);
    assert_eq!(f.app.pointer.surface, Some(Vec2::new(200.0, 100.0)));
    assert_eq!(f.app.session().publication_context(), publication);
    assert!(f.app.rebind_pointer_view().is_err());
    assert_eq!(f.app.pointer.view_revision, 0);
    f.app
        .static_session_mut()
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
    f.edge(ElementState::Pressed);
    assert_eq!(f.value(f.down), &ReactiveValue::Scalar(1.0));
}

#[test]
fn idle_callback_only_program_does_not_activate_pointer_ingress() {
    let (program, callbacks) =
        noon::example_scenes::ordinary_callback_continuation_program().unwrap();
    let source = LiveProgramExecutionSource::new(program, callbacks).unwrap();
    let mut app = NativeApp::from_source(Box::new(source), NativeViewportConfig::default());
    assert!(!app.execution.session().has_native_pointer_subscribers());
    app.rebind_pointer_view().unwrap();
    app.dispatch_pointer_position(PhysicalPosition::new(200.0, 100.0), SIZE, 1.0)
        .unwrap();
    app.dispatch_pointer_button(MouseButton::Left, ElementState::Pressed, SIZE, 1.0)
        .unwrap();
    app.pointer_focus_lost().unwrap();
    assert!(!app.pointer.configured);
    assert_eq!(app.next_input_sequence, 0);
    app.advance_realtime_timeline(std::time::Instant::now())
        .unwrap();
    assert_eq!(app.session().frame().time, 0.0);
}

#[test]
fn mapping_preserves_camera_offset_and_outside_surface_coordinates() {
    let camera = Camera2DState {
        center: Vec2::new(2.0, -1.0),
        height: 6.0,
    };
    let p = pointer_position(Vec2::new(900.0, -100.0), Vec2::new(800.0, 400.0), camera).unwrap();
    assert_eq!(p.surface(), Vec2::new(900.0, -100.0));
    assert_eq!(p.scene(), Vec2::new(9.5, 3.5));
}

#[test]
fn exhausted_sequence_and_view_revision_never_wrap_or_clear_a_press() {
    let mut f = Fixture::new();
    f.move_to(200.0, 100.0);
    f.edge(ElementState::Pressed);
    let before = f.app.session().publication_context();
    f.app.pointer.view_revision = u64::MAX;
    assert!(f.app.rebind_pointer_view().is_err());
    f.app.next_input_sequence = u64::MAX;
    assert!(f.app.pointer_focus_lost().is_err());
    assert_eq!(f.app.session().publication_context(), before);
    assert_eq!(f.value(f.button), &ReactiveValue::Bool(true));
}

#[test]
fn event_only_pointer_interest_includes_the_full_button_vocabulary() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let down = signal(
        &mut store,
        root,
        SemanticNativeInputSource::Event(NativeEventSource::PointerDown { button: 255 }),
        SemanticSignalValue::Scalar(0.0),
    );
    let session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    assert!(session.has_native_pointer_subscribers());
    let mut app = NativeApp::new(session, NativeViewportConfig::default());
    app.dispatch_pointer_position(PhysicalPosition::new(200.0, 100.0), SIZE, 1.0)
        .unwrap();
    app.dispatch_pointer_button(MouseButton::Other(255), ElementState::Pressed, SIZE, 1.0)
        .unwrap();
    assert_eq!(
        app.session().effective_signal_value(down),
        Some(&ReactiveValue::Scalar(1.0))
    );
    let seq = app.next_input_sequence;
    assert!(app
        .dispatch_pointer_button(MouseButton::Other(256), ElementState::Pressed, SIZE, 1.0)
        .is_err());
    assert_eq!(app.next_input_sequence, seq);
}
