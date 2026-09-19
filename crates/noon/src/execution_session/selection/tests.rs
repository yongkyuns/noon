use super::*;
use crate::integration::NativePointerInputPublication;
use noon_core::{
    AnimationOptions, NativeEventSource, NativeInputModifiers, NativePointerCancellation,
    NativePointerId, NativePointerPosition, NativeStateSource, RateFunction, ReactiveValue,
    SemanticMutationTransaction, SemanticNativeInputSource, SemanticNodeCreation,
    SemanticObjectProperty, SemanticObjectState, SemanticSignalExpr, SemanticSignalValue,
    SemanticStore, SemanticVec3, StoredGeometry, Vec2, VectorPath,
};

const POINTER: NativePointerId = NativePointerId {
    source: 3,
    pointer: 8,
};
const TOLERANCE: f32 = 4.0;

struct Fixture {
    store: SemanticStore,
    root: SemanticNodeId,
    target: SemanticNodeId,
    behind: SemanticNodeId,
}
impl Fixture {
    fn new() -> Self {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let behind =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Rectangle {
                size: Vec2::new(2.0, 2.0),
            }));
        let target =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        for node in [behind, target] {
            store.add_semantic_family_member(root, node).unwrap();
        }
        Self {
            store,
            root,
            target,
            behind,
        }
    }
    fn native(
        &mut self,
        source: SemanticNativeInputSource,
        initial: SemanticSignalValue,
    ) -> SemanticNodeId {
        let mut tx = SemanticMutationTransaction::new();
        let pending =
            tx.create_node(SemanticNodeCreation::native_input_signal(initial, source).unwrap());
        tx.scope_signal(self.root, pending);
        tx.apply(&mut self.store).unwrap().resolve(pending).unwrap()
    }
    fn events(&mut self) -> (SemanticNodeId, SemanticNodeId, SemanticNodeId) {
        let down = self.native(
            SemanticNativeInputSource::Event(NativeEventSource::PointerDown { button: 0 }),
            SemanticSignalValue::Scalar(0.0),
        );
        let up = self.native(
            SemanticNativeInputSource::Event(NativeEventSource::PointerUp { button: 0 }),
            SemanticSignalValue::Scalar(0.0),
        );
        let held = self.native(
            SemanticNativeInputSource::State(NativeStateSource::PointerButton { button: 0 }),
            SemanticSignalValue::Bool(false),
        );
        (down, up, held)
    }
    fn session(&self) -> ExecutionSession {
        let mut s = ExecutionSession::from_semantic_root(&self.store, self.root).unwrap();
        s.configure_native_pointer_selection(TOLERANCE).unwrap();
        s.configure_native_pointer_input(POINTER, 1).unwrap();
        s.take_frame_changes();
        s
    }
}
fn position(scene: Vec2, surface: Vec2) -> NativePointerPosition {
    NativePointerPosition::new(scene, surface).unwrap()
}
fn edge(pressed: bool, scene: Vec2, surface: Vec2, button: u8) -> NativePointerInputKind {
    let position = position(scene, surface);
    if pressed {
        NativePointerInputKind::Press { position, button }
    } else {
        NativePointerInputKind::Release { position, button }
    }
}
fn input(
    token: &NativePointerInputToken,
    sequence: u64,
    kind: NativePointerInputKind,
) -> NativePointerInput {
    NativePointerInput::new(
        sequence,
        token.pointer(),
        token.context(),
        NativeInputModifiers {
            shift: true,
            ..Default::default()
        },
        kind,
    )
}
fn submit(
    s: &mut ExecutionSession,
    sequence: u64,
    kind: NativePointerInputKind,
) -> NativePointerInputPublication {
    let token = s.native_pointer_input_token().unwrap();
    s.submit_native_pointer_input(&token, input(&token, sequence, kind))
        .unwrap()
}
fn press(s: &mut ExecutionSession, seq: u64, point: Vec2) -> NativePointerInputPublication {
    submit(s, seq, edge(true, point, Vec2::ZERO, 0))
}
fn release(s: &mut ExecutionSession, seq: u64, point: Vec2) -> NativePointerInputPublication {
    submit(s, seq, edge(false, point, Vec2::ZERO, 0))
}
fn click(s: &mut ExecutionSession, seq: u64, point: Vec2) -> NativePointerInputPublication {
    press(s, seq, point);
    release(s, seq + 1, point)
}

#[test]
fn selection_is_opt_in_and_contributes_native_collector_interest() {
    let f = Fixture::new();
    let mut s = ExecutionSession::from_semantic_root(&f.store, f.root).unwrap();
    s.configure_native_pointer_input(POINTER, 1).unwrap();
    s.take_frame_changes();
    assert!(!s.has_native_pointer_subscribers());
    let result = click(&mut s, 0, Vec2::ZERO);
    assert_eq!(result.selection_click(), None);
    assert_eq!(result.selection_query(), None);
    assert_eq!(s.native_pointer_selection(), None);
    assert!(s.wake_state().is_quiescent());
    s.configure_native_pointer_selection(TOLERANCE).unwrap();
    assert!(s.has_native_pointer_subscribers());
    s.disable_native_pointer_selection().unwrap();
    assert!(!s.has_native_pointer_subscribers());
}

#[test]
fn paused_click_selects_topmost_without_changing_authored_frame_or_export_rows() {
    let f = Fixture::new();
    let mut s = f.session();
    let before = s.frame().clone();
    let publication = s.publication_context();
    let revision = f.store.scene_revision();
    let order = s.painter_order().to_vec();
    let down = press(&mut s, 0, Vec2::ZERO);
    assert_eq!(down.selection_click(), None);
    assert!(!down.selection_changed());
    assert_eq!(
        down.selection_query().unwrap().outcome(),
        PointerFillOutcome::Hit(f.target)
    );
    assert!(s.wake_state().is_quiescent());
    let up = release(&mut s, 1, Vec2::ZERO);
    let recognized = up.selection_click().unwrap();
    assert_eq!(recognized.target(), Some(f.target));
    assert_eq!(recognized.press(), down.input());
    assert_eq!(recognized.release(), up.input());
    assert!(recognized.press().modifiers().shift);
    assert!(up.selection_changed());
    let selection = s.native_pointer_selection().unwrap();
    assert_eq!(selection.target(), f.target);
    assert_eq!(selection.publication(), publication);
    assert_eq!(
        selection.bounds(),
        Some(Rect::new(Vec2::new(-1.0, -1.0), Vec2::new(1.0, 1.0)))
    );
    assert_eq!(s.publication_context(), publication);
    assert_eq!(s.frame(), &before);
    assert_eq!(f.store.scene_revision(), revision);
    assert_eq!(s.painter_order(), order);
    assert!(s.wake_state().frame_pending());
    let ordinary = s.take_renderer_publication();
    assert!(
        ordinary.transient_presentations().is_empty(),
        "selection is not authored or ordinary export content"
    );
    drop(ordinary);
    assert!(s.wake_state().is_quiescent());
    assert_eq!(s.native_pointer_selection(), Some(selection));
}

#[test]
fn selection_requests_presentation_only_redraw_and_preserves_other_dirty_rows() {
    let mut f = Fixture::new();
    let signal = f
        .store
        .insert_semantic_input_signal(SemanticVec3::ZERO)
        .unwrap();
    f.store
        .bind_semantic_signal(signal, f.target, SemanticObjectProperty::Translation)
        .unwrap();
    let mut s = f.session();
    s.set_reactive_input(signal, Vec2::new(3.0, 0.0)).unwrap();
    click(&mut s, 0, Vec2::new(3.0, 0.0));
    let changes = s.take_frame_changes();
    assert!(changes.requires_presentation_redraw());
    assert_eq!(changes.object_indices(), &[1]);
    assert!(!changes.is_all());
    assert!(!changes.is_structural());
    s.disable_native_pointer_selection().unwrap();
    let erased = s.take_frame_changes();
    assert!(erased.requires_presentation_redraw());
    assert!(erased.object_indices().is_empty());
    assert!(!erased.has_stable_changes());
    assert!(s.wake_state().is_quiescent());
}

#[test]
fn circle_bounds_false_positive_selects_precise_rectangle_behind() {
    let f = Fixture::new();
    let mut s = f.session();
    let result = click(&mut s, 0, Vec2::new(0.9, 0.9));
    assert_eq!(result.selection_click().unwrap().target(), Some(f.behind));
    assert_eq!(result.selection_query().unwrap().precise_tests(), 2);
}

#[test]
fn out_and_back_motion_disarms_without_coalescing_or_picking_each_move() {
    let f = Fixture::new();
    let mut s = f.session();
    press(&mut s, 0, Vec2::ZERO);
    for (seq, surface) in [(1, Vec2::new(20.0, 0.0)), (2, Vec2::ZERO)] {
        let motion = submit(
            &mut s,
            seq,
            NativePointerInputKind::Move(position(Vec2::ZERO, surface)),
        );
        assert_eq!(motion.selection_query(), None);
        assert_eq!(motion.selection_click(), None);
    }
    let up = release(&mut s, 3, Vec2::ZERO);
    assert_eq!(up.selection_click(), None);
    assert_eq!(up.selection_query(), None);
    assert_eq!(s.native_pointer_selection(), None);
    assert!(s.wake_state().is_quiescent());
    assert!(click(&mut s, 4, Vec2::ZERO).selection_click().is_some());
}

#[test]
fn movement_tolerance_uses_logical_surface_pixels_and_checks_release() {
    for (distance, accepts) in [(4.0, true), (4.001, false)] {
        let f = Fixture::new();
        let mut s = f.session();
        press(&mut s, 0, Vec2::ZERO);
        let result = submit(
            &mut s,
            1,
            edge(false, Vec2::ZERO, Vec2::new(distance, 0.0), 0),
        );
        assert_eq!(result.selection_click().is_some(), accepts);
    }
    let f = Fixture::new();
    let mut s = f.session();
    s.configure_native_pointer_selection(0.0).unwrap();
    assert!(click(&mut s, 0, Vec2::ZERO).selection_click().is_some());
}

#[test]
fn release_must_match_pressed_target_and_background_click_can_clear_selection() {
    let f = Fixture::new();
    let mut s = f.session();
    click(&mut s, 0, Vec2::ZERO);
    press(&mut s, 2, Vec2::ZERO);
    assert!(release(&mut s, 3, Vec2::new(0.9, 0.9))
        .selection_click()
        .is_none());
    assert_eq!(s.native_pointer_selection().unwrap().target(), f.target);
    press(&mut s, 4, Vec2::ZERO);
    assert!(release(&mut s, 5, Vec2::new(10.0, 10.0))
        .selection_click()
        .is_none());
    press(&mut s, 6, Vec2::new(10.0, 10.0));
    assert!(release(&mut s, 7, Vec2::ZERO).selection_click().is_none());
    let background = click(&mut s, 8, Vec2::new(10.0, 10.0));
    assert_eq!(background.selection_click().unwrap().target(), None);
    assert!(background.selection_changed());
    assert_eq!(s.native_pointer_selection(), None);
}

#[test]
fn duplicate_and_chorded_edges_remain_native_events_but_do_not_synthesize_clicks() {
    let mut f = Fixture::new();
    let (down, up, held) = f.events();
    let mut s = f.session();
    press(&mut s, 0, Vec2::ZERO);
    press(&mut s, 1, Vec2::ZERO);
    assert!(release(&mut s, 2, Vec2::ZERO).selection_click().is_none());
    assert!(release(&mut s, 3, Vec2::ZERO).selection_click().is_none());
    assert_eq!(
        s.effective_signal_value(down),
        Some(&ReactiveValue::Scalar(2.0))
    );
    assert_eq!(
        s.effective_signal_value(up),
        Some(&ReactiveValue::Scalar(2.0))
    );
    assert_eq!(
        s.effective_signal_value(held),
        Some(&ReactiveValue::Bool(false))
    );
    press(&mut s, 4, Vec2::ZERO);
    submit(&mut s, 5, edge(true, Vec2::ZERO, Vec2::ZERO, 255));
    submit(&mut s, 6, edge(false, Vec2::ZERO, Vec2::ZERO, 255));
    assert!(release(&mut s, 7, Vec2::ZERO).selection_click().is_none());
    let result = click(&mut s, 8, Vec2::ZERO);
    assert!(result.selection_click().is_some());
    assert!(release(&mut s, 10, Vec2::ZERO).selection_click().is_none());
    assert_eq!(
        s.effective_signal_value(down),
        Some(&ReactiveValue::Scalar(4.0))
    );
    assert_eq!(
        s.effective_signal_value(up),
        Some(&ReactiveValue::Scalar(5.0))
    );
}

#[test]
fn every_cancellation_reason_disarms_without_clearing_existing_selection_or_releasing() {
    for reason in [
        NativePointerCancellation::Cancelled,
        NativePointerCancellation::CaptureLost,
        NativePointerCancellation::CaptureFailed,
        NativePointerCancellation::FocusLost,
    ] {
        let mut f = Fixture::new();
        let (_, up, held) = f.events();
        let mut s = f.session();
        click(&mut s, 0, Vec2::ZERO);
        press(&mut s, 2, Vec2::ZERO);
        let cancellation = submit(&mut s, 3, NativePointerInputKind::Cancel(reason));
        assert_eq!(cancellation.selection_query(), None);
        assert_eq!(cancellation.selection_click(), None);
        assert_eq!(s.native_pointer_selection().unwrap().target(), f.target);
        assert_eq!(
            s.effective_signal_value(held),
            Some(&ReactiveValue::Bool(false))
        );
        assert_eq!(
            s.effective_signal_value(up),
            Some(&ReactiveValue::Scalar(1.0))
        );
        assert!(release(&mut s, 4, Vec2::ZERO).selection_click().is_none());
    }
}

#[test]
fn view_rebinding_and_retired_source_cancellation_cannot_complete_or_break_new_gesture() {
    let f = Fixture::new();
    let mut s = f.session();
    press(&mut s, 0, Vec2::ZERO);
    let retired = s.native_pointer_input_token().unwrap();
    s.configure_native_pointer_input(POINTER, 2).unwrap();
    assert!(release(&mut s, 1, Vec2::ZERO).selection_click().is_none());
    press(&mut s, 2, Vec2::ZERO);
    let old_cancel = input(
        &retired,
        3,
        NativePointerInputKind::Cancel(NativePointerCancellation::FocusLost),
    );
    assert_eq!(
        s.submit_native_pointer_input(&retired, old_cancel),
        Err(ExecutionSessionInputError::StalePointerBinding)
    );
    assert!(release(&mut s, 3, Vec2::ZERO).selection_click().is_some());
}

#[test]
fn foreign_pointer_and_runtime_failures_do_not_disarm_current_gesture() {
    let f = Fixture::new();
    let mut s = f.session();
    press(&mut s, 0, Vec2::ZERO);
    let token = s.native_pointer_input_token().unwrap();
    let foreign = NativePointerInput::new(
        1,
        NativePointerId {
            source: 8,
            pointer: 8,
        },
        token.context(),
        NativeInputModifiers::default(),
        NativePointerInputKind::Cancel(NativePointerCancellation::FocusLost),
    );
    assert!(matches!(
        s.submit_native_pointer_input(&token, foreign),
        Err(ExecutionSessionInputError::WrongPointer { .. })
    ));
    let other = f.session();
    let token2 = other.native_pointer_input_token().unwrap();
    assert_eq!(
        s.submit_native_pointer_input(
            &token2,
            input(
                &token2,
                1,
                NativePointerInputKind::Move(position(Vec2::ZERO, Vec2::new(100.0, 0.0)))
            )
        ),
        Err(ExecutionSessionInputError::ForeignPointerRuntime)
    );
    assert!(release(&mut s, 1, Vec2::ZERO).selection_click().is_some());
}

#[test]
fn rejected_current_source_motion_disarms_to_prevent_missing_path_evidence_click() {
    let f = Fixture::new();
    let mut s = f.session();
    press(&mut s, 0, Vec2::ZERO);
    let old = s.native_pointer_input_token().unwrap();
    s.advance_to(1.0).unwrap();
    s.take_frame_changes();
    let before = s.publication_context();
    let stale_motion = input(
        &old,
        1,
        NativePointerInputKind::Move(position(Vec2::ZERO, Vec2::new(100.0, 0.0))),
    );
    assert!(matches!(
        s.submit_native_pointer_input(&old, stale_motion),
        Err(ExecutionSessionInputError::StalePointerPublication { .. })
    ));
    assert_eq!(s.publication_context(), before);
    assert_eq!(s.last_native_event_sequence, Some(0));
    assert!(s.take_frame_changes().is_empty());
    assert!(release(&mut s, 1, Vec2::ZERO).selection_click().is_none());
}

#[test]
fn callback_stall_neither_admits_input_nor_bypasses_selection_policy() {
    let f = Fixture::new();
    let mut s = f.session();
    press(&mut s, 0, Vec2::ZERO);
    let token = s.native_pointer_input_token().unwrap();
    let overlay = s.begin_required_callback_phase(0.0, [f.target]).unwrap();
    let before = s.publication_context();
    for sequence in 1..513 {
        assert_eq!(
            s.submit_native_pointer_input(
                &token,
                input(&token, sequence, edge(false, Vec2::ZERO, Vec2::ZERO, 0))
            ),
            Err(ExecutionSessionInputError::RequiredCallbackPending)
        );
    }
    assert_eq!(
        s.configure_native_pointer_selection(3.0),
        Err(ExecutionSessionInputError::RequiredCallbackPending)
    );
    assert_eq!(s.last_native_event_sequence, Some(0));
    assert_eq!(s.publication_context(), before);
    assert_eq!(s.native_pointer_selection(), None);
    s.commit_required_callback_phase(overlay.finish()).unwrap();
    assert!(release(&mut s, 1, Vec2::ZERO).selection_click().is_none());
    assert!(click(&mut s, 2, Vec2::ZERO).selection_click().is_some());
}

#[test]
fn failed_reactive_release_does_not_commit_selection_and_disarms_candidate() {
    let mut f = Fixture::new();
    let (down, up, held) = f.events();
    let gain = f.store.insert_semantic_input_signal(1.0e20_f64).unwrap();
    let scaled = f
        .store
        .insert_semantic_derived_signal(SemanticSignalExpr::Mul(
            Box::new(SemanticSignalExpr::signal(up)),
            Box::new(SemanticSignalExpr::signal(gain)),
        ))
        .unwrap();
    let squared = f
        .store
        .insert_semantic_derived_signal(SemanticSignalExpr::Mul(
            Box::new(SemanticSignalExpr::signal(scaled)),
            Box::new(SemanticSignalExpr::signal(scaled)),
        ))
        .unwrap();
    f.store
        .bind_semantic_signal(squared, f.behind, SemanticObjectProperty::RotationZ)
        .unwrap();
    let mut s = f.session();
    press(&mut s, 0, Vec2::ZERO);
    let token = s.native_pointer_input_token().unwrap();
    let before = s.publication_context();
    s.take_frame_changes();
    assert!(matches!(
        s.submit_native_pointer_input(
            &token,
            input(&token, 1, edge(false, Vec2::ZERO, Vec2::ZERO, 0))
        ),
        Err(ExecutionSessionInputError::Evaluation(_))
    ));
    assert_eq!(s.publication_context(), before);
    assert_eq!(s.last_native_event_sequence, Some(0));
    assert_eq!(
        s.effective_signal_value(down),
        Some(&ReactiveValue::Scalar(1.0))
    );
    assert_eq!(
        s.effective_signal_value(up),
        Some(&ReactiveValue::Scalar(0.0))
    );
    assert_eq!(
        s.effective_signal_value(held),
        Some(&ReactiveValue::Bool(true))
    );
    assert_eq!(s.native_pointer_selection(), None);
    assert!(s.take_frame_changes().is_empty());
    s.set_reactive_input(gain, 1.0_f32).unwrap();
    assert!(release(&mut s, 1, Vec2::ZERO).selection_click().is_none());
    assert!(click(&mut s, 2, Vec2::ZERO).selection_click().is_some());
}

#[test]
fn release_picks_before_reactive_publication_and_selection_observes_after_state() {
    let mut f = Fixture::new();
    let (_, up, _) = f.events();
    f.store
        .bind_semantic_signal(up, f.behind, SemanticObjectProperty::RotationZ)
        .unwrap();
    let mut s = f.session();
    let result = click(&mut s, 0, Vec2::new(0.9, 0.9));
    assert_eq!(result.selection_click().unwrap().target(), Some(f.behind));
    assert_eq!(
        result.selection_query().unwrap().publication(),
        result.previous_publication()
    );
    assert_ne!(result.publication(), result.previous_publication());
    let selection = s.native_pointer_selection().unwrap();
    assert_eq!(selection.publication(), result.publication());
    assert!(selection.bounds().unwrap().width() > 2.0);
    assert_eq!(s.frame().objects[0].transform.rotation, 1.0);
}

#[test]
fn animation_moves_selection_bounds_without_changing_target_or_requiring_pointer_motion() {
    let mut f = Fixture::new();
    let mut target = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    target.transform.translation = SemanticVec3::new(8.0, 0.0, 0.0);
    let target = f.store.insert_semantic_object(target);
    let animation = f
        .store
        .insert_semantic_transform_animation(f.target, target, AnimationOptions::new())
        .unwrap();
    let mut s = f.session();
    s.activate_animation_segment(
        &f.store,
        animation,
        AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    )
    .unwrap();
    click(&mut s, 0, Vec2::ZERO);
    s.take_frame_changes();
    let selected = s.native_pointer_selection().unwrap();
    s.advance_to(1.0).unwrap();
    let moved = s.native_pointer_selection().unwrap();
    assert_eq!(moved.target(), selected.target());
    assert_eq!(moved.bounds().unwrap().center(), Vec2::new(4.0, 0.0));
    assert_ne!(moved.publication(), selected.publication());
    assert_eq!(s.frame().time, 1.0);
    assert!(!s.take_frame_changes().object_indices().is_empty());
}

#[test]
fn removal_replacement_and_structural_revision_retire_old_target_and_press() {
    let mut f = Fixture::new();
    let mut s = f.session();
    click(&mut s, 0, Vec2::ZERO);
    press(&mut s, 2, Vec2::ZERO);
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(f.target);
    s.apply_semantic_transaction(&mut f.store, tx).unwrap();
    assert_eq!(s.native_pointer_selection(), None);
    let mut tx = SemanticMutationTransaction::new();
    let replacement = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    tx.add_member(f.root, replacement);
    let result = s.apply_semantic_transaction(&mut f.store, tx).unwrap();
    let replacement = result.resolve(replacement).unwrap();
    assert_ne!(replacement, f.target);
    assert!(release(&mut s, 3, Vec2::ZERO).selection_click().is_none());
    assert_eq!(
        click(&mut s, 4, Vec2::ZERO)
            .selection_click()
            .unwrap()
            .target(),
        Some(replacement)
    );
}

#[test]
fn clone_and_successful_seek_clear_transients_but_failed_seek_does_not() {
    let f = Fixture::new();
    let mut s = f.session();
    click(&mut s, 0, Vec2::ZERO);
    press(&mut s, 2, Vec2::ZERO);
    let mut clone = s.clone();
    assert!(clone.has_native_pointer_subscribers());
    assert_eq!(clone.native_pointer_selection(), None);
    assert!(release(&mut clone, 3, Vec2::ZERO)
        .selection_click()
        .is_none());
    let selected = s.native_pointer_selection();
    assert!(s.seek(f64::NAN).is_err());
    assert_eq!(s.native_pointer_selection(), selected);
    assert!(release(&mut s, 3, Vec2::ZERO).selection_click().is_some());
    press(&mut s, 4, Vec2::ZERO);
    s.seek(0.0).unwrap();
    assert_eq!(s.native_pointer_selection(), None);
    assert!(release(&mut s, 5, Vec2::ZERO).selection_click().is_none());
    assert!(s.has_native_pointer_subscribers());
}

#[test]
fn invalid_tool_configuration_is_atomic_and_disable_retains_native_subscriptions() {
    let mut f = Fixture::new();
    f.events();
    let mut s = f.session();
    click(&mut s, 0, Vec2::ZERO);
    press(&mut s, 2, Vec2::ZERO);
    let before = s.native_pointer_selection();
    for tolerance in [-1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            s.configure_native_pointer_selection(tolerance),
            Err(ExecutionSessionInputError::InvalidSelectionTolerance)
        );
        assert_eq!(s.native_pointer_selection(), before);
    }
    assert!(release(&mut s, 3, Vec2::ZERO).selection_click().is_some());
    s.disable_native_pointer_selection().unwrap();
    assert!(s.has_native_pointer_subscribers());
    assert_eq!(s.native_pointer_selection(), None);
    assert_eq!(click(&mut s, 4, Vec2::ZERO).selection_click(), None);
}

#[test]
fn unsupported_filled_candidate_blocks_selection_instead_of_selecting_behind_it() {
    let mut f = Fixture::new();
    let path = VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, -1.0))
        .line_to(Vec2::new(0.0, 1.0))
        .close();
    let resource = f.store.insert_geometry_path(path).unwrap();
    let path = f
        .store
        .insert_semantic_object(SemanticObjectState::new(StoredGeometry::Resource(resource)));
    f.store.add_semantic_family_member(f.root, path).unwrap();
    let mut s = f.session();
    let down = press(&mut s, 0, Vec2::ZERO);
    assert!(
        matches!(down.selection_query().unwrap().outcome(), PointerFillOutcome::Unsupported { target, .. } if target == path)
    );
    assert!(release(&mut s, 1, Vec2::ZERO).selection_click().is_none());
    assert_eq!(s.native_pointer_selection(), None);
}

#[test]
fn ten_thousand_unrelated_objects_leave_click_work_candidate_local() {
    let mut f = Fixture::new();
    for i in 0..10_000 {
        let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.5 });
        state.transform.translation = SemanticVec3::new(100.0 + f64::from(i) * 3.0, 0.0, 0.0);
        let node = f.store.insert_semantic_object(state);
        f.store.add_semantic_family_member(f.root, node).unwrap();
    }
    let mut s = f.session();
    let down = press(&mut s, 0, Vec2::ZERO);
    let result = down.selection_query().unwrap();
    assert_eq!(result.spatial_stats().results, 2);
    assert_eq!(result.precise_tests(), 1);
    assert_eq!(result.spatial_stats().full_scan_fallbacks, 0);
    let up = release(&mut s, 1, Vec2::ZERO);
    assert_eq!(up.selection_query().unwrap().precise_tests(), 1);
    assert_eq!(s.last_spatial_update_stats().leaves_upserted, 0);
    assert_eq!(s.last_spatial_update_stats().full_rebuilds, 0);
    for _ in 0..100 {
        assert_eq!(s.native_pointer_selection().unwrap().target(), f.target);
    }
    let changes = s.take_frame_changes();
    assert!(changes.requires_presentation_redraw());
    assert!(changes.object_indices().is_empty());
    assert_eq!(s.frame().time, 0.0);
}
