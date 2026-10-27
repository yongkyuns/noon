use super::*;
use crate::browser_pointer_input::BrowserPointerKind;
use noon_core::{
    NativeEventSource, NativeInputValue, NativeStateSource, ReactiveValue,
    SemanticMutationTransaction, SemanticNativeInputSource, SemanticNodeCreation, SemanticNodeId,
    SemanticSignalValue, SemanticStore, SemanticVec3,
};

const SIZE: Vec2 = Vec2::new(800.0, 400.0);
fn input(kind: BrowserPointerKind) -> BrowserPointerInput {
    let cancel = matches!(
        kind,
        BrowserPointerKind::Cancel
            | BrowserPointerKind::FocusLost
            | BrowserPointerKind::CaptureLost
    );
    BrowserPointerInput {
        kind,
        source_id: 1,
        pointer_id: 7,
        view_revision: 1,
        surface_x: (!cancel).then_some(400.0),
        surface_y: (!cancel).then_some(200.0),
        viewport_width: (!cancel).then_some(SIZE.x),
        viewport_height: (!cancel).then_some(SIZE.y),
        button: matches!(
            kind,
            BrowserPointerKind::Press | BrowserPointerKind::Release
        )
        .then_some(0),
        shift: false,
        control: false,
        alt: false,
        meta: false,
    }
}

struct Fixture {
    execution: noon::SceneExecution,
    host: DirectPointerPresentation,
    binding: Option<BrowserPointerBinding>,
    sequence: u64,
    target: SemanticNodeId,
}
impl Fixture {
    fn new() -> Self {
        let mut scene = noon::Scene::new();
        let mut circle = scene.circle(1.0).unwrap();
        circle.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
        scene.add(&circle).unwrap();
        let target = circle.node_id();
        let mut execution = scene.into_execution().unwrap();
        execution
            .session_mut()
            .enable_pointer_fill_selection(4.0)
            .unwrap();
        let mut host = DirectPointerPresentation::default();
        host.set_view(1, SIZE).unwrap();
        Self {
            execution,
            host,
            binding: None,
            sequence: 0,
            target,
        }
    }
    fn present(&mut self) {
        let camera = self.execution.session().camera().unwrap();
        let frame = self.host.capture(&mut self.execution, camera).unwrap();
        self.execution.session_mut().take_renderer_publication();
        self.host.did_present(frame);
    }
    fn send(&mut self, wire: BrowserPointerInput) -> Result<bool, String> {
        self.host
            .submit(
                &mut self.execution,
                &mut self.binding,
                &mut self.sequence,
                wire,
            )
            .map(|publication| publication.is_some())
    }
    fn click(&mut self) {
        assert!(self.send(input(BrowserPointerKind::Press)).unwrap());
        assert!(self.send(input(BrowserPointerKind::Release)).unwrap());
    }
}

#[test]
fn capture_alone_never_authorizes_input_or_consumes_publication() {
    let mut f = Fixture::new();
    let before = f.execution.session().frame().clone();
    let wake = f.execution.session().wake_state();
    let camera = f.execution.session().camera().unwrap();
    let captured = f.host.capture(&mut f.execution, camera).unwrap();
    assert!(captured.is_some());
    assert!(!f.send(input(BrowserPointerKind::Press)).unwrap());
    assert!(f.binding.is_none());
    assert_eq!(f.sequence, 0);
    assert_eq!(f.execution.session().frame(), &before);
    assert_eq!(f.execution.session().wake_state(), wake);
    assert!(f.host.refresh_pending);
}

#[test]
fn successful_receipt_authorizes_click_without_authored_changes() {
    let mut f = Fixture::new();
    f.present();
    let before = f.execution.session().frame().clone();
    let publication = f.execution.session().publication_context();
    f.click();
    assert_eq!(
        f.execution.session().selected_pointer_target(),
        Some(f.target)
    );
    assert_eq!(f.execution.session().frame(), &before);
    assert_eq!(f.execution.session().publication_context(), publication);
    assert!(!f.host.refresh_pending);
    assert!(f
        .execution
        .session_mut()
        .take_renderer_publication()
        .changes()
        .is_empty());
}

#[test]
fn dormant_collector_does_not_configure_callback_input_or_request_redraw() {
    let mut scene = noon::Scene::new();
    let circle = scene.circle(1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(
        circle.node_id(),
        noon_core::HostCallbackId::new(1),
        0.0,
        None,
    );
    transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let mut f = Fixture::new();
    f.execution = scene.into_execution().unwrap();
    f.present();
    let publication = f.execution.session().publication_context();
    assert!(f.send(input(BrowserPointerKind::Cancel)).is_err());
    assert!(!f.send(input(BrowserPointerKind::Move)).unwrap());
    assert!(f.binding.is_none());
    assert_eq!(f.sequence, 0);
    assert_eq!(f.execution.session().publication_context(), publication);
    assert!(!f.host.refresh_pending);
    assert!(f
        .execution
        .session_mut()
        .take_renderer_publication()
        .changes()
        .is_empty());
}

#[test]
fn consumed_but_unpresented_round_trip_rejects_and_cancels_stationary_press() {
    let mut f = Fixture::new();
    f.present();
    assert!(f.send(input(BrowserPointerKind::Press)).unwrap());
    let displayed = f.host.presented.clone();
    f.execution.session_mut().evaluate(0.25).unwrap();
    f.execution.seek(0.0).unwrap();
    assert_ne!(
        f.execution.session().publication_context(),
        displayed.as_ref().unwrap().publication()
    );
    f.execution.session_mut().take_renderer_publication();
    assert!(!f.send(input(BrowserPointerKind::Release)).unwrap());
    assert!(f.execution.session().selected_pointer_target().is_none());
    assert_eq!(
        f.sequence, 2,
        "only press plus a separate cancellation were admitted"
    );
    assert_eq!(f.host.presented, displayed);
    assert!(f.host.refresh_pending);
    f.present();
    assert!(
        f.send(input(BrowserPointerKind::Release)).is_err(),
        "retired release cannot revive the gesture"
    );
    for kind in [BrowserPointerKind::Press, BrowserPointerKind::Release] {
        let mut wire = input(kind);
        wire.source_id = 2;
        assert!(f.send(wire).unwrap());
    }
    assert_eq!(
        f.execution.session().selected_pointer_target(),
        Some(f.target)
    );
}

#[test]
fn same_time_seek_requires_fresh_presentation_even_with_unchanged_publication() {
    let mut f = Fixture::new();
    f.present();
    f.click();
    let publication = f.execution.session().publication_context();
    f.execution.seek(0.0).unwrap();
    assert_eq!(f.execution.session().publication_context(), publication);
    assert!(f.execution.session().selected_pointer_target().is_none());
    assert!(f.host.needs_refresh(f.execution.session()));
    assert!(!f.send(input(BrowserPointerKind::Press)).unwrap());
    assert!(f.host.refresh_pending);
    f.present();
    for kind in [BrowserPointerKind::Press, BrowserPointerKind::Release] {
        let mut wire = input(kind);
        wire.source_id = 2;
        assert!(f.send(wire).unwrap());
    }
    assert_eq!(
        f.execution.session().selected_pointer_target(),
        Some(f.target)
    );
}

#[test]
fn seek_retired_capture_cannot_be_replaced_during_admission() {
    let mut f = Fixture::new();
    f.present();
    f.execution.seek(0.25).unwrap();
    let publication = f.execution.session().publication_context();
    assert!(!f.send(input(BrowserPointerKind::Press)).unwrap());
    assert_eq!(f.execution.session().publication_context(), publication);
    assert_eq!(f.sequence, 0);
    assert!(f.binding.is_none());
}

#[test]
fn view_revision_or_size_change_requires_new_presentation() {
    for resized in [false, true] {
        let mut f = Fixture::new();
        f.present();
        let size = if resized {
            Vec2::new(400.0, 200.0)
        } else {
            SIZE
        };
        assert!(f.host.set_view(2, size).unwrap());
        assert!(f.host.presented.is_none());
        let mut wire = input(BrowserPointerKind::Press);
        wire.view_revision = 2;
        wire.viewport_width = Some(size.x);
        wire.viewport_height = Some(size.y);
        assert!(!f.send(wire).unwrap());
        f.present();
        assert!(f.send(wire).unwrap());
    }
}

#[test]
fn mismatched_occurrence_view_cannot_configure_a_source() {
    for change_size in [false, true] {
        let mut f = Fixture::new();
        f.present();
        let mut wire = input(BrowserPointerKind::Press);
        if change_size {
            wire.viewport_width = Some(801.0);
        } else {
            wire.view_revision += 1;
        }
        assert!(!f.send(wire).unwrap());
        assert!(f.binding.is_none());
        assert_eq!(f.sequence, 0);
    }
}

#[test]
fn unchanged_view_is_a_constant_time_noop() {
    let mut f = Fixture::new();
    f.present();
    let receipt = f.host.presented.clone();
    for _ in 0..128 {
        assert!(!f.host.set_view(1, SIZE).unwrap());
    }
    assert_eq!(f.host.presented, receipt);
    assert!(!f.host.refresh_pending);
}

#[test]
fn hidden_view_retires_receipt_and_preserves_revision_high_watermark() {
    let mut f = Fixture::new();
    f.present();
    f.host.set_view(9, Vec2::ZERO).unwrap();
    assert!(f.host.viewport().is_none());
    let camera = f.execution.session().camera().unwrap();
    assert!(f.host.capture(&mut f.execution, camera).unwrap().is_none());
    assert!(f.host.set_view(8, SIZE).is_err());
    assert!(!f.send(input(BrowserPointerKind::Press)).unwrap());
}

#[test]
fn malformed_view_does_not_destroy_a_valid_receipt() {
    let mut f = Fixture::new();
    f.present();
    let receipt = f.host.presented.clone();
    for size in [
        Vec2::new(f32::NAN, 2.0),
        Vec2::new(-1.0, 2.0),
        Vec2::new(2.0, f32::INFINITY),
    ] {
        assert!(f.host.set_view(2, size).is_err());
    }
    assert!(f.host.set_view(1_u64 << 53, SIZE).is_err());
    assert_eq!(f.host.presented, receipt);
}

#[test]
fn invalid_coordinates_remain_errors_without_a_receipt() {
    let mut f = Fixture::new();
    let mut wire = input(BrowserPointerKind::Press);
    wire.surface_x = Some(f32::NAN);
    assert!(f.send(wire).is_err());
    assert_eq!(f.sequence, 0);
    assert!(f.binding.is_none());
}

#[test]
fn surface_invalidation_does_not_acknowledge_the_rejected_edge() {
    let mut f = Fixture::new();
    f.present();
    assert!(f.send(input(BrowserPointerKind::Press)).unwrap());
    f.host.invalidate();
    assert!(!f.send(input(BrowserPointerKind::Release)).unwrap());
    assert_eq!(f.sequence, 2);
    assert!(f.execution.session().selected_pointer_target().is_none());
}

#[test]
fn cancellation_without_a_receipt_preserves_ordinary_source_retirement() {
    for kind in [
        BrowserPointerKind::Cancel,
        BrowserPointerKind::FocusLost,
        BrowserPointerKind::CaptureLost,
    ] {
        let mut f = Fixture::new();
        f.present();
        assert!(f.send(input(BrowserPointerKind::Press)).unwrap());
        f.host.invalidate();
        assert!(f.send(input(kind)).unwrap());
        assert_eq!(f.sequence, 2);
        f.present();
        assert!(f.send(input(BrowserPointerKind::Release)).is_err());
    }
}

#[test]
fn manual_camera_mismatch_cannot_mint_a_receipt_or_spin_forever() {
    let mut f = Fixture::new();
    let camera = Camera2DState {
        center: Vec2::new(2.0, 0.0),
        height: 8.0,
    };
    assert!(f.host.capture(&mut f.execution, camera).unwrap().is_none());
    f.host.did_present(None);
    assert!(!f.host.refresh_pending);
    assert!(!f.send(input(BrowserPointerKind::Press)).unwrap());
    assert!(f.host.refresh_pending);
}

#[test]
fn snapshot_from_another_runtime_is_a_recoverable_rejection() {
    let mut f = Fixture::new();
    f.present();
    let other_runtime = f.execution.session().clone();
    *f.execution.session_mut() = other_runtime;
    assert!(!f.send(input(BrowserPointerKind::Press)).unwrap());
    assert_eq!(f.sequence, 0);
}

#[test]
fn required_callback_barrier_is_not_misclassified_as_display_race() {
    let mut f = Fixture::new();
    f.present();
    let overlay = f
        .execution
        .session_mut()
        .begin_required_callback_phase(0.0, [f.target])
        .unwrap();
    assert!(f.send(input(BrowserPointerKind::Press)).is_err());
    assert_eq!(f.sequence, 0);
    assert!(!f.host.refresh_pending);
    f.execution
        .session_mut()
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
}

fn signal(
    store: &mut SemanticStore,
    root: SemanticNodeId,
    source: NativeStateSource,
    initial: SemanticSignalValue,
) -> SemanticNodeId {
    let mut tx = SemanticMutationTransaction::new();
    let pending = tx.create_node(
        SemanticNodeCreation::native_input_signal(
            initial,
            SemanticNativeInputSource::State(source),
        )
        .unwrap(),
    );
    tx.scope_signal(root, pending);
    tx.apply(store).unwrap().resolve(pending).unwrap()
}

fn event_signal(
    store: &mut SemanticStore,
    root: SemanticNodeId,
    source: NativeEventSource,
) -> SemanticNodeId {
    let mut transaction = SemanticMutationTransaction::new();
    let pending = transaction.create_node(
        SemanticNodeCreation::native_input_signal(
            SemanticSignalValue::Scalar(0.0),
            SemanticNativeInputSource::Event(source),
        )
        .unwrap(),
    );
    transaction.scope_signal(root, pending);
    transaction.apply(store).unwrap().resolve(pending).unwrap()
}

#[test]
fn signal_only_binding_reset_preserves_the_collected_context_and_delivers_once() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let button = signal(
        &mut store,
        root,
        NativeStateSource::PointerButton { button: 0 },
        SemanticSignalValue::Bool(false),
    );
    let position = signal(
        &mut store,
        root,
        NativeStateSource::PointerPosition,
        SemanticSignalValue::Vec3(SemanticVec3::ZERO),
    );
    let down = event_signal(
        &mut store,
        root,
        NativeEventSource::PointerDown { button: 0 },
    );
    let scene =
        noon::Scene::with_integration_root(std::rc::Rc::new(std::cell::RefCell::new(store)), root)
            .unwrap();
    let mut execution = scene.into_execution().unwrap();
    execution
        .session_mut()
        .set_native_state_input(
            NativeStateSource::PointerButton { button: 0 },
            NativeInputValue::Bool(true),
        )
        .unwrap();
    let mut host = DirectPointerPresentation::default();
    host.set_view(1, SIZE).unwrap();
    let camera = execution.session().camera().unwrap();
    let frame = host.capture(&mut execution, camera).unwrap();
    host.did_present(frame);
    let displayed = host.presented.clone().unwrap();
    let mut binding = None;
    let mut sequence = 0;
    let mut wire = input(BrowserPointerKind::Press);
    wire.surface_x = Some(600.0);
    let publication = host
        .submit(&mut execution, &mut binding, &mut sequence, wire)
        .unwrap()
        .expect("a reset of unrendered signals preserves the displayed pick");
    assert_eq!(
        publication.input().context().publication,
        displayed.publication()
    );
    assert_ne!(publication.previous_publication(), displayed.publication());
    assert_eq!(
        execution.session().effective_signal_value(button).unwrap(),
        &ReactiveValue::Bool(true)
    );
    assert_eq!(
        execution
            .session()
            .effective_signal_value(position)
            .unwrap(),
        &ReactiveValue::Vec2(Vec2::new(4.0, 0.0))
    );
    assert_eq!(
        execution.session().effective_signal_value(down).unwrap(),
        &ReactiveValue::Scalar(1.0)
    );
    assert_eq!(sequence, 1);
    assert_eq!(host.presented, Some(displayed));
    assert!(!host.refresh_pending);
}

#[test]
fn binding_reset_that_removes_the_displayed_target_rejects_the_press() {
    let mut fixture = noon::example_scenes::pointer_input_trace::Fixture::signals_only();
    fixture
        .store
        .bind_semantic_signal(
            fixture.button,
            fixture.target,
            noon_core::SemanticObjectProperty::Presence,
        )
        .unwrap();
    let scene = noon::Scene::with_integration_root(
        std::rc::Rc::new(std::cell::RefCell::new(fixture.store)),
        fixture.root,
    )
    .unwrap();
    let mut execution = scene.into_execution().unwrap();
    execution
        .session_mut()
        .enable_pointer_fill_selection(4.0)
        .unwrap();
    execution
        .session_mut()
        .set_native_state_input(
            NativeStateSource::PointerButton { button: 0 },
            NativeInputValue::Bool(true),
        )
        .unwrap();
    let mut host = DirectPointerPresentation::default();
    host.set_view(1, SIZE).unwrap();
    let camera = execution.session().camera().unwrap();
    let frame = host.capture(&mut execution, camera).unwrap();
    host.did_present(frame);
    let displayed = host.presented.clone();
    let mut binding = None;
    let mut sequence = 0;

    assert!(host
        .submit(
            &mut execution,
            &mut binding,
            &mut sequence,
            input(BrowserPointerKind::Press),
        )
        .unwrap()
        .is_none());
    assert_eq!(
        execution.session().effective_signal_value(fixture.button),
        Some(&ReactiveValue::Bool(false))
    );
    assert_eq!(
        execution.session().effective_signal_value(fixture.down),
        Some(&ReactiveValue::Scalar(0.0))
    );
    assert_eq!(
        execution.session().effective_signal_value(fixture.position),
        Some(&ReactiveValue::Vec2(Vec2::ZERO))
    );
    assert_eq!(
        sequence, 1,
        "only the separate cleanup cancellation is accepted"
    );
    assert_eq!(host.presented, displayed);
    assert!(host.refresh_pending);
    assert!(execution.session().selected_pointer_target().is_none());
}

#[test]
fn exhausted_sequence_is_not_a_recoverable_frame_outcome() {
    for presented in [false, true] {
        let mut f = Fixture::new();
        if presented {
            f.present();
        }
        let refresh = f.host.refresh_pending;
        f.sequence = u64::MAX;
        assert!(f.send(input(BrowserPointerKind::Press)).is_err());
        assert!(f.binding.is_none());
        assert_eq!(f.host.refresh_pending, refresh);
    }
}

#[test]
fn missing_receipt_cannot_relax_retired_sources_or_invent_initial_release() {
    let mut f = Fixture::new();
    assert!(f.send(input(BrowserPointerKind::Release)).is_err());
    f.present();
    assert!(f.send(input(BrowserPointerKind::Press)).unwrap());
    assert!(f.send(input(BrowserPointerKind::Cancel)).unwrap());
    f.host.invalidate();
    let sequence = f.sequence;
    assert!(f.send(input(BrowserPointerKind::Release)).is_err());
    assert_eq!(f.sequence, sequence);
}

#[test]
fn surface_repaint_without_input_cannot_revive_a_stationary_gesture() {
    let mut f = Fixture::new();
    f.present();
    let authored = f.execution.session().frame().clone();
    assert!(f.send(input(BrowserPointerKind::Press)).unwrap());
    f.host.invalidate();
    browser_pointer_input::cancel_browser_pointer_input(
        &mut f.execution,
        &mut f.binding,
        &mut f.sequence,
        false,
    )
    .unwrap();
    assert_eq!(
        f.sequence, 2,
        "press and cancellation only; no invented release"
    );
    // No occurrence is offered while the surface is unavailable. A matching
    // replacement receipt must not make the original press eligible again.
    f.present();
    assert!(f.send(input(BrowserPointerKind::Release)).unwrap());
    assert_eq!(f.sequence, 3);
    assert_eq!(f.execution.session().selected_pointer_target(), None);
    assert_eq!(f.execution.session().frame(), &authored);
    for kind in [BrowserPointerKind::Press, BrowserPointerKind::Release] {
        let mut wire = input(kind);
        wire.source_id = 2;
        assert!(f.send(wire).unwrap());
    }
    assert_eq!(
        f.execution.session().selected_pointer_target(),
        Some(f.target)
    );
}

#[test]
fn surface_gesture_cleanup_preserves_callback_and_sequence_barriers() {
    let mut f = Fixture::new();
    f.present();
    assert!(f.send(input(BrowserPointerKind::Press)).unwrap());
    let binding = f.binding;
    let sequence = f.sequence;
    let phase = f
        .execution
        .session_mut()
        .begin_required_callback_phase(0.0, [f.target])
        .unwrap();
    assert!(browser_pointer_input::cancel_browser_pointer_input(
        &mut f.execution,
        &mut f.binding,
        &mut f.sequence,
        false,
    )
    .is_err());
    assert_eq!(f.binding, binding);
    assert_eq!(f.sequence, sequence);
    f.execution
        .session_mut()
        .commit_required_callback_phase(phase.finish())
        .unwrap();
    f.sequence = u64::MAX;
    assert!(browser_pointer_input::cancel_browser_pointer_input(
        &mut f.execution,
        &mut f.binding,
        &mut f.sequence,
        false,
    )
    .is_err());
    assert_eq!(f.binding, binding);
    assert_eq!(f.sequence, u64::MAX);
    f.sequence = sequence;
    browser_pointer_input::cancel_browser_pointer_input(
        &mut f.execution,
        &mut f.binding,
        &mut f.sequence,
        false,
    )
    .unwrap();
    f.present();
    assert!(f.send(input(BrowserPointerKind::Release)).unwrap());
    assert_eq!(f.execution.session().selected_pointer_target(), None);
}

#[test]
fn surface_cleanup_does_not_relax_explicit_source_retirement() {
    let mut f = Fixture::new();
    f.present();
    assert!(f.send(input(BrowserPointerKind::Press)).unwrap());
    assert!(f.send(input(BrowserPointerKind::Cancel)).unwrap());
    let sequence = f.sequence;
    browser_pointer_input::cancel_browser_pointer_input(
        &mut f.execution,
        &mut f.binding,
        &mut f.sequence,
        false,
    )
    .unwrap();
    assert_eq!(f.sequence, sequence);
    f.present();
    assert!(f.send(input(BrowserPointerKind::Release)).is_err());
}

#[test]
fn signal_only_publication_stays_quiet_without_pointer_subscribers() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let source = NativeStateSource::Control {
        name: "receipt-only".into(),
    };
    signal(
        &mut store,
        root,
        source.clone(),
        SemanticSignalValue::Scalar(0.0),
    );
    let scene =
        noon::Scene::with_integration_root(std::rc::Rc::new(std::cell::RefCell::new(store)), root)
            .unwrap();
    let mut execution = scene.into_execution().unwrap();
    let mut host = DirectPointerPresentation::default();
    host.set_view(1, SIZE).unwrap();
    let camera = execution.session().camera().unwrap();
    let frame = host.capture(&mut execution, camera).unwrap();
    execution.session_mut().take_renderer_publication();
    host.did_present(frame);
    let displayed = execution.session().publication_context();
    assert!(!host.needs_refresh(execution.session()));
    execution
        .session_mut()
        .set_native_state_input(source, NativeInputValue::Scalar(1.0))
        .unwrap();
    assert_ne!(displayed, execution.session().publication_context());
    assert!(execution
        .session_mut()
        .take_renderer_publication()
        .changes()
        .is_empty());
    assert!(!execution.session().wake_state().frame_pending());
    for _ in 0..128 {
        assert!(!host.needs_refresh(execution.session()));
        assert!(
            !execution.session().wake_state().frame_pending(),
            "a dormant pointer view must not request receipt-only refreshes"
        );
    }
    assert_eq!(
        host.presented.as_ref().unwrap().publication(),
        displayed,
        "quiet publication changes must not rewrite the dormant receipt"
    );
}

#[test]
fn signal_only_publication_preserves_collected_context_and_ordered_edges() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let source = NativeStateSource::Control {
        name: "receipt-only".into(),
    };
    signal(
        &mut store,
        root,
        source.clone(),
        SemanticSignalValue::Scalar(0.0),
    );
    let down = event_signal(
        &mut store,
        root,
        NativeEventSource::PointerDown { button: 0 },
    );
    let up = event_signal(&mut store, root, NativeEventSource::PointerUp { button: 0 });
    let scene =
        noon::Scene::with_integration_root(std::rc::Rc::new(std::cell::RefCell::new(store)), root)
            .unwrap();
    let mut execution = scene.into_execution().unwrap();
    execution
        .session_mut()
        .enable_pointer_fill_selection(4.0)
        .unwrap();
    assert!(execution.session().has_native_pointer_subscribers());
    let mut host = DirectPointerPresentation::default();
    host.set_view(1, SIZE).unwrap();
    let camera = execution.session().camera().unwrap();
    let frame = host.capture(&mut execution, camera).unwrap();
    execution.session_mut().take_renderer_publication();
    host.did_present(frame);
    let displayed = host.presented.clone().unwrap();

    execution
        .session_mut()
        .set_native_state_input(source, NativeInputValue::Scalar(1.0))
        .unwrap();
    assert_ne!(
        displayed.publication(),
        execution.session().publication_context()
    );
    assert!(execution
        .session_mut()
        .take_renderer_publication()
        .changes()
        .is_empty());
    assert!(host.needs_refresh(execution.session()));

    let mut binding = None;
    let mut sequence = 0;
    for kind in [BrowserPointerKind::Press, BrowserPointerKind::Release] {
        let before_input = execution.session().publication_context();
        let publication = host
            .submit(&mut execution, &mut binding, &mut sequence, input(kind))
            .unwrap()
            .expect("unchanged displayed picking admits the original occurrence");
        assert_eq!(
            publication.input().context().publication,
            displayed.publication()
        );
        assert_eq!(publication.previous_publication(), before_input);
        assert_ne!(publication.previous_publication(), displayed.publication());
    }
    assert_eq!(
        execution.session().effective_signal_value(down),
        Some(&ReactiveValue::Scalar(1.0))
    );
    assert_eq!(
        execution.session().effective_signal_value(up),
        Some(&ReactiveValue::Scalar(1.0))
    );
    assert_eq!(sequence, 2, "each original edge is delivered exactly once");
    assert_eq!(host.presented, Some(displayed));
    assert!(!host.refresh_pending);
}

#[test]
fn callback_barrier_does_not_create_a_receipt_only_redraw_loop() {
    let mut f = Fixture::new();
    f.present();
    let phase = f
        .execution
        .session_mut()
        .begin_required_callback_phase(0.0, [f.target])
        .unwrap();
    assert!(!f.host.needs_refresh(f.execution.session()));
    f.execution
        .session_mut()
        .commit_required_callback_phase(phase.finish())
        .unwrap();
}

mod inspection;

#[test]
fn direct_static_pointer_motion_observes_hover_without_a_redraw() {
    let mut f = Fixture::new();
    f.present();
    let publication = f.execution.session().publication_context();
    assert!(f.send(input(BrowserPointerKind::Move)).unwrap());
    assert_eq!(
        f.execution.session().hovered_pointer_target(),
        Some(f.target)
    );
    let mut outside = input(BrowserPointerKind::Move);
    outside.surface_x = Some(700.0);
    assert!(f.send(outside).unwrap());
    assert_eq!(f.execution.session().hovered_pointer_target(), None);
    assert_eq!(f.execution.session().publication_context(), publication);
    assert!(!f.host.needs_refresh(f.execution.session()));
    assert_eq!(f.execution.session().frame().time, 0.0);
}
