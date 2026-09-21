use noon_core::{
    AnimationOptions, NativeEventSource, NativeInputModifiers, NativePointerCancellation,
    NativePointerId, NativePointerPosition, RateFunction, SemanticMutationTransaction,
    SemanticNativeInputSource, SemanticNodeCreation, SemanticObjectProperty, SemanticObjectState,
    SemanticSignalExpr, SemanticSignalValue, SemanticStore, SemanticVec3, StoredGeometry, Vec2,
    VectorPath,
};

use super::*;

const POINTER: NativePointerId = NativePointerId {
    source: 8,
    pointer: 3,
};

fn circle(x: f64) -> SemanticObjectState {
    let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    state.transform.translation = SemanticVec3::new(x, 0.0, 0.0);
    state
}

fn attach(
    store: &mut SemanticStore,
    root: SemanticNodeId,
    state: SemanticObjectState,
) -> SemanticNodeId {
    let node = store.insert_semantic_object(state);
    store.add_semantic_family_member(root, node).unwrap();
    node
}

fn session(store: &SemanticStore, root: SemanticNodeId) -> ExecutionSession {
    let mut session = ExecutionSession::from_semantic_root(store, root).unwrap();
    session.configure_native_pointer_input(POINTER, 1).unwrap();
    session.enable_pointer_fill_selection(5.0).unwrap();
    session.take_frame_changes();
    session
}

fn fixture() -> (
    SemanticStore,
    SemanticNodeId,
    SemanticNodeId,
    ExecutionSession,
) {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let target = attach(&mut store, root, circle(0.0));
    let session = session(&store, root);
    (store, root, target, session)
}

fn position(x: f32, surface_x: f32) -> NativePointerPosition {
    NativePointerPosition::new(Vec2::new(x, 0.0), Vec2::new(surface_x, 100.0)).unwrap()
}

fn press(x: f32, surface_x: f32) -> NativePointerInputKind {
    NativePointerInputKind::Press {
        position: position(x, surface_x),
        button: 0,
    }
}

fn release(x: f32, surface_x: f32) -> NativePointerInputKind {
    NativePointerInputKind::Release {
        position: position(x, surface_x),
        button: 0,
    }
}

fn input(
    session: &ExecutionSession,
    sequence: u64,
    kind: NativePointerInputKind,
) -> (NativePointerInputToken, NativePointerInput) {
    let token = session.native_pointer_input_token().unwrap();
    let input = NativePointerInput::new(
        sequence,
        token.pointer(),
        token.context(),
        NativeInputModifiers {
            shift: true,
            ..Default::default()
        },
        kind,
    );
    (token, input)
}

fn submit(
    session: &mut ExecutionSession,
    sequence: u64,
    kind: NativePointerInputKind,
) -> super::super::NativePointerInputPublication {
    let (token, input) = input(session, sequence, kind);
    session.submit_native_pointer_input(&token, input).unwrap()
}

fn click(
    session: &mut ExecutionSession,
    sequence: u64,
    x: f32,
) -> super::super::NativePointerInputPublication {
    submit(session, sequence, press(x, 100.0));
    submit(session, sequence + 1, release(x, 100.0))
}

#[test]
fn selection_is_explicit_opt_in_and_extends_collector_interest_without_scene_changes() {
    let (_, _, target, mut session) = fixture();
    session.disable_pointer_fill_selection().unwrap();
    assert!(!session.has_native_pointer_subscribers());
    let before = session.frame().clone();
    let context = session.publication_context();
    assert_eq!(click(&mut session, 0, 0.0).selection_click(), None);
    assert_eq!(session.selected_pointer_target(), None);
    session.enable_pointer_fill_selection(5.0).unwrap();
    assert!(session.has_native_pointer_subscribers());
    assert_eq!(
        click(&mut session, 2, 0.0)
            .selection_click()
            .unwrap()
            .target(),
        Some(target)
    );
    assert_eq!(session.publication_context(), context);
    assert_eq!(session.frame(), &before);
    session.disable_pointer_fill_selection().unwrap();
    assert_eq!(session.selected_pointer_target(), None);
}

#[test]
fn admitted_click_keeps_both_occurrences_and_does_not_contaminate_scene_rendering() {
    let (store, _, target, mut session) = fixture();
    let revision = store.scene_revision();
    let before = session.frame().clone();
    let context = session.publication_context();
    let order = session.take_renderer_publication().painter_order().to_vec();
    let down = submit(&mut session, 10, press(0.0, 100.0));
    assert_eq!(
        down.selection_query().unwrap().outcome(),
        PointerFillOutcome::Hit(target)
    );
    assert_eq!(session.selected_pointer_target(), None);
    let up = submit(&mut session, 11, release(0.01, 101.0));
    let occurrence = up.selection_click().unwrap();
    assert!(up.selection_changed());
    assert_eq!(occurrence.press(), down.input());
    assert_eq!(occurrence.release(), up.input());
    assert_eq!(occurrence.target(), Some(target));
    assert_eq!(occurrence.press().position().unwrap().surface().x, 100.0);
    assert!(occurrence.press().modifiers().shift);
    assert_eq!(session.selected_pointer_target(), Some(target));
    let highlight = session.pointer_selection_highlight().unwrap();
    assert_eq!(highlight.target, target);
    assert_eq!(highlight.publication, context);
    assert_eq!(highlight.geometry, GeometryRef::Circle { radius: 1.0 });
    assert_eq!(session.frame(), &before);
    assert_eq!(session.publication_context(), context);
    assert_eq!(store.scene_revision(), revision);
    let publication = session.take_renderer_publication();
    assert!(publication.changes().is_empty());
    assert_eq!(publication.painter_order(), order);
    assert!(publication.transient_presentations().is_empty());
    assert!(session.wake_state().is_quiescent());
    let extra_release = submit(&mut session, 12, release(0.01, 101.0));
    assert_eq!(extra_release.selection_click(), None);
    assert!(!extra_release.selection_changed());
    let repeated_click = click(&mut session, 13, 0.0);
    assert!(repeated_click.selection_click().is_some());
    assert!(!repeated_click.selection_changed());
}

#[test]
fn every_motion_sample_contributes_and_out_and_back_cannot_become_a_click() {
    let (_, _, _, mut session) = fixture();
    submit(&mut session, 0, press(0.0, 100.0));
    let out = submit(
        &mut session,
        1,
        NativePointerInputKind::Move(position(0.1, 110.0)),
    );
    let back = submit(
        &mut session,
        2,
        NativePointerInputKind::Move(position(0.0, 100.0)),
    );
    assert_eq!(
        out.selection_query(),
        None,
        "motion must not run a picking query"
    );
    assert_eq!(back.selection_query(), None);
    let up = submit(&mut session, 3, release(0.0, 100.0));
    assert_eq!(up.selection_click(), None);
    assert_eq!(up.selection_query(), None);
    assert_eq!(session.selected_pointer_target(), None);
}

#[test]
fn radial_tolerance_is_inclusive_and_uses_surface_pixels_not_scene_distance() {
    for (surface, expected) in [
        (Vec2::new(103.0, 104.0), true),
        (Vec2::new(103.0, 104.01), false),
    ] {
        let (_, _, _, mut session) = fixture();
        submit(&mut session, 0, press(0.0, 100.0));
        let kind = NativePointerInputKind::Release {
            position: NativePointerPosition::new(Vec2::new(0.001, 0.0), surface).unwrap(),
            button: 0,
        };
        assert_eq!(
            submit(&mut session, 1, kind).selection_click().is_some(),
            expected
        );
    }
    let (mut store, root, _, _) = fixture();
    let mut huge = circle(0.0);
    huge.transform.scale = SemanticVec3::new(1000.0, 1000.0, 1.0);
    let target = attach(&mut store, root, huge);
    let mut session = session(&store, root);
    submit(&mut session, 0, press(0.0, 100.0));
    assert_eq!(
        submit(&mut session, 1, release(100.0, 101.0))
            .selection_click()
            .unwrap()
            .target(),
        Some(target)
    );
}

#[test]
fn finite_extreme_surface_coordinates_do_not_overflow_distance_into_acceptance() {
    let (_, _, _, mut session) = fixture();
    session.enable_pointer_fill_selection(f32::MAX).unwrap();
    submit(&mut session, 0, press(0.0, -f32::MAX));
    assert_eq!(
        submit(&mut session, 1, release(0.0, f32::MAX)).selection_click(),
        None
    );
}

#[test]
fn release_repicks_the_current_topmost_fill_and_rejects_mismatched_targets() {
    let (mut store, root, original, _) = fixture();
    let other = attach(&mut store, root, circle(4.0));
    let mut session = session(&store, root);
    click(&mut session, 0, 0.0);
    submit(&mut session, 2, press(0.0, 100.0));
    let up = submit(&mut session, 3, release(4.0, 101.0));
    assert_eq!(
        up.selection_query().unwrap().outcome(),
        PointerFillOutcome::Hit(other)
    );
    assert_eq!(up.selection_click(), None);
    assert_eq!(session.selected_pointer_target(), Some(original));
    submit(&mut session, 4, press(0.0, 100.0));
    assert_eq!(
        submit(&mut session, 5, release(2.0, 101.0)).selection_click(),
        None
    );
    assert_eq!(session.selected_pointer_target(), Some(original));
    let clear = click(&mut session, 6, 2.0);
    assert_eq!(clear.selection_click().unwrap().target(), None);
    assert!(clear.selection_changed());
    assert_eq!(session.pointer_selection_highlight(), None);
}

#[test]
fn click_uses_precise_fill_not_the_topmost_bounding_box() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let bottom = attach(
        &mut store,
        root,
        SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(2.0, 2.0),
        }),
    );
    let top = attach(&mut store, root, circle(0.0));
    let mut session = session(&store, root);
    let point = NativePointerPosition::new(Vec2::new(0.9, 0.9), Vec2::new(100.0, 100.0)).unwrap();
    submit(
        &mut session,
        0,
        NativePointerInputKind::Press {
            position: point,
            button: 0,
        },
    );
    let up = submit(
        &mut session,
        1,
        NativePointerInputKind::Release {
            position: point,
            button: 0,
        },
    );
    assert_eq!(up.selection_click().unwrap().target(), Some(bottom));
    assert_eq!(up.selection_query().unwrap().precise_tests(), 2);
    assert_eq!(
        click(&mut session, 2, 0.0)
            .selection_click()
            .unwrap()
            .target(),
        Some(top)
    );
}

#[test]
fn all_cancellations_end_the_gesture_without_erasing_existing_selection() {
    for reason in [
        NativePointerCancellation::Cancelled,
        NativePointerCancellation::CaptureLost,
        NativePointerCancellation::CaptureFailed,
        NativePointerCancellation::FocusLost,
    ] {
        let (_, _, target, mut session) = fixture();
        click(&mut session, 0, 0.0);
        submit(&mut session, 2, press(0.0, 100.0));
        submit(&mut session, 3, NativePointerInputKind::Cancel(reason));
        assert_eq!(
            submit(&mut session, 4, release(0.0, 100.0)).selection_click(),
            None
        );
        assert_eq!(session.selected_pointer_target(), Some(target));
    }
}

#[test]
fn duplicate_down_and_chords_do_not_rearm_an_active_gesture() {
    for button in [0, 1, 2, 255] {
        let (_, _, _, mut session) = fixture();
        submit(&mut session, 0, press(0.0, 100.0));
        submit(
            &mut session,
            1,
            NativePointerInputKind::Press {
                position: position(0.0, 100.0),
                button,
            },
        );
        assert_eq!(
            submit(&mut session, 2, release(0.0, 100.0)).selection_click(),
            None
        );
    }
    let (_, _, _, mut session) = fixture();
    submit(
        &mut session,
        0,
        NativePointerInputKind::Press {
            position: position(0.0, 100.0),
            button: 255,
        },
    );
    assert_eq!(
        click(&mut session, 1, 0.0).selection_click(),
        None,
        "a secondary button already held also disqualifies primary"
    );
    submit(
        &mut session,
        3,
        NativePointerInputKind::Cancel(NativePointerCancellation::Cancelled),
    );
    assert!(click(&mut session, 4, 0.0).selection_click().is_some());
}

#[test]
fn source_or_view_rebinding_cancels_pending_click_and_foreign_input_is_atomic() {
    let (_, _, _, mut session) = fixture();
    submit(&mut session, 0, press(0.0, 100.0));
    let before = session.pointer_selection;
    let (token, original) = input(&session, 1, release(0.0, 100.0));
    let foreign = NativePointerInput::new(
        1,
        NativePointerId {
            source: 9,
            pointer: 3,
        },
        token.context(),
        Default::default(),
        original.kind(),
    );
    assert!(matches!(
        session.submit_native_pointer_input(&token, foreign),
        Err(ExecutionSessionInputError::WrongPointer { .. })
    ));
    assert_eq!(session.pointer_selection, before);
    session.configure_native_pointer_input(POINTER, 2).unwrap();
    assert!(matches!(
        session.submit_native_pointer_input(&token, original),
        Err(ExecutionSessionInputError::StalePointerBinding)
    ));
    assert_eq!(
        submit(&mut session, 1, release(0.0, 100.0)).selection_click(),
        None
    );
}

#[test]
fn rejected_motion_followed_by_a_sequence_gap_cannot_become_a_click() {
    let (_, _, _, mut session) = fixture();
    submit(&mut session, 0, press(0.0, 100.0));
    let (stale, movement) = input(
        &session,
        1,
        NativePointerInputKind::Move(position(0.1, 120.0)),
    );
    session.advance_to(1.0).unwrap();
    let before = session.pointer_selection;
    assert!(matches!(
        session.submit_native_pointer_input(&stale, movement),
        Err(ExecutionSessionInputError::StalePointerPublication { .. })
    ));
    assert_eq!(session.pointer_selection, before);
    assert_eq!(session.last_native_event_sequence, Some(0));
    assert_eq!(
        submit(&mut session, 2, release(0.0, 100.0)).selection_click(),
        None
    );
}

#[test]
fn callback_barrier_rejects_selection_configuration_and_input_without_partial_changes() {
    let (_, _, target, mut session) = fixture();
    submit(&mut session, 0, press(0.0, 100.0));
    let before = session.pointer_selection;
    let (token, up) = input(&session, 1, release(0.0, 100.0));
    let overlay = session
        .begin_required_callback_phase(0.0, [target])
        .unwrap();
    assert_eq!(
        session.enable_pointer_fill_selection(4.0),
        Err(ExecutionSessionInputError::RequiredCallbackPending)
    );
    assert_eq!(
        session.disable_pointer_fill_selection(),
        Err(ExecutionSessionInputError::RequiredCallbackPending)
    );
    assert_eq!(
        session.submit_native_pointer_input(&token, up),
        Err(ExecutionSessionInputError::RequiredCallbackPending)
    );
    assert_eq!(session.pointer_selection, before);
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
}

#[test]
fn removal_replacement_and_even_unrelated_revision_changes_invalidate_selection() {
    let (mut store, root, old, mut session) = fixture();
    click(&mut session, 0, 0.0);
    submit(&mut session, 2, press(0.0, 100.0));
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(old);
    session
        .apply_semantic_transaction(&mut store, remove)
        .unwrap();
    assert_eq!(session.selected_pointer_target(), None);
    let mut add = SemanticMutationTransaction::new();
    let pending = add.create_node(SemanticNodeCreation::object(circle(0.0)));
    add.add_member(root, pending);
    let replacement = session
        .apply_semantic_transaction(&mut store, add)
        .unwrap()
        .resolve(pending)
        .unwrap();
    assert_ne!(old, replacement);
    assert_eq!(session.pointer_selection_highlight(), None);
    assert_eq!(
        submit(&mut session, 3, release(0.0, 100.0)).selection_click(),
        None
    );
    assert_eq!(
        click(&mut session, 4, 0.0)
            .selection_click()
            .unwrap()
            .target(),
        Some(replacement)
    );
    let mut add = SemanticMutationTransaction::new();
    let other = add.create_node(SemanticNodeCreation::object(circle(20.0)));
    add.add_member(root, other);
    session.apply_semantic_transaction(&mut store, add).unwrap();
    assert_eq!(
        session.selected_pointer_target(),
        None,
        "initial policy conservatively invalidates across any authored revision"
    );
}

#[test]
fn clone_seek_and_backward_advance_clear_transient_state_only_after_success() {
    let (_, _, target, mut session) = fixture();
    click(&mut session, 0, 0.0);
    submit(&mut session, 2, press(0.0, 100.0));
    let before = session.pointer_selection;
    let clone = session.clone();
    assert_eq!(clone.selected_pointer_target(), None);
    assert!(clone.has_native_pointer_subscribers());
    assert_eq!(clone.pointer_selection.pending, None);
    assert_eq!(session.selected_pointer_target(), Some(target));
    assert!(session.seek(f64::NAN).is_err());
    assert_eq!(session.pointer_selection, before);
    session.seek(0.0).unwrap();
    assert_eq!(session.selected_pointer_target(), None);
    assert_eq!(
        submit(&mut session, 3, release(0.0, 100.0)).selection_click(),
        None
    );
    session.advance_to(1.0).unwrap();
    click(&mut session, 4, 0.0);
    session.advance_to(0.0).unwrap();
    assert_eq!(session.pointer_selection.pending, None);
    assert_eq!(session.pointer_selection_highlight(), None);
}

#[test]
fn animated_geometry_is_repicked_at_release_and_highlight_follows_effective_transform() {
    let (mut store, root, target, _) = fixture();
    let endpoint = store.insert_semantic_object(circle(4.0));
    let animation = store
        .insert_semantic_transform_animation(target, endpoint, AnimationOptions::new())
        .unwrap();
    let mut session = session(&store, root);
    session
        .activate_animation_segment(
            &store,
            animation,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    session.take_frame_changes();
    submit(&mut session, 0, press(0.0, 100.0));
    session.advance_to(1.0).unwrap();
    assert_eq!(
        submit(&mut session, 1, release(0.0, 100.0)).selection_click(),
        None,
        "the target moved away from the release point"
    );
    assert_eq!(
        click(&mut session, 2, 2.0)
            .selection_click()
            .unwrap()
            .target(),
        Some(target)
    );
    let before = session.pointer_selection_highlight().unwrap();
    assert_eq!(before.transform.translation, Vec2::new(2.0, 0.0));
    session.take_frame_changes();
    session.advance_to(1.5).unwrap();
    let highlight = session.pointer_selection_highlight().unwrap();
    assert_eq!(highlight.target, target);
    assert_eq!(highlight.transform.translation, Vec2::new(3.0, 0.0));
    assert_ne!(highlight.publication, before.publication);
    assert!(
        !session.take_frame_changes().is_empty(),
        "highlight observation must not consume renderer changes"
    );
}

#[test]
fn undecidable_fill_does_not_click_through_or_clear_an_existing_selection() {
    let (mut store, root, _, _) = fixture();
    let path = store
        .insert_geometry_path(
            VectorPath::new()
                .move_to(Vec2::new(-1.0, -1.0))
                .line_to(Vec2::new(1.0, -1.0))
                .line_to(Vec2::new(0.0, 1.0))
                .close(),
        )
        .unwrap();
    let top = attach(
        &mut store,
        root,
        SemanticObjectState::new(StoredGeometry::Resource(path)),
    );
    let other = attach(&mut store, root, circle(4.0));
    let mut session = session(&store, root);
    click(&mut session, 0, 4.0);
    let down = submit(&mut session, 2, press(0.0, 100.0));
    assert!(
        matches!(down.selection_query().unwrap().outcome(), PointerFillOutcome::Unsupported { target, .. } if target == top)
    );
    let up = submit(&mut session, 3, release(0.0, 100.0));
    assert_eq!(up.selection_click(), None);
    assert_eq!(session.selected_pointer_target(), Some(other));
}

#[test]
fn invalid_configuration_does_not_reset_an_existing_gesture_or_selection() {
    let (_, _, _, mut session) = fixture();
    click(&mut session, 0, 0.0);
    submit(&mut session, 2, press(0.0, 100.0));
    let before = session.pointer_selection;
    for value in [-1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            session.enable_pointer_fill_selection(value),
            Err(ExecutionSessionInputError::InvalidPointerClickTolerance)
        );
        assert_eq!(session.pointer_selection, before);
    }
    assert!(submit(&mut session, 3, release(0.0, 100.0))
        .selection_click()
        .is_some());
}

#[test]
fn ten_thousand_object_click_queries_only_local_candidates_and_no_motion_or_overlay_scan() {
    let (mut store, root, target, _) = fixture();
    for i in 0..10_000 {
        attach(&mut store, root, circle(20.0 + f64::from(i) * 6.0));
    }
    let mut session = session(&store, root);
    let down = submit(&mut session, 0, press(0.0, 100.0));
    for sequence in 1..=100 {
        assert_eq!(
            submit(
                &mut session,
                sequence,
                NativePointerInputKind::Move(position(0.0, 100.0))
            )
            .selection_query(),
            None
        );
    }
    let up = submit(&mut session, 101, release(0.0, 100.0));
    for query in [
        down.selection_query().unwrap(),
        up.selection_query().unwrap(),
    ] {
        assert_eq!(query.outcome(), PointerFillOutcome::Hit(target));
        assert_eq!(query.precise_tests(), 1);
        assert_eq!(query.spatial_stats().candidates_tested, 1);
        assert_eq!(query.spatial_stats().full_scan_fallbacks, 0);
    }
    let spatial = session.last_spatial_update;
    for _ in 0..100 {
        assert_eq!(
            session.pointer_selection_highlight().unwrap().target,
            target
        );
    }
    assert_eq!(session.last_spatial_update, spatial);
    assert!(session.take_frame_changes().is_empty());
}

fn failing_edge_fixture(
    event: NativeEventSource,
) -> (
    SemanticStore,
    SemanticNodeId,
    SemanticNodeId,
    SemanticNodeId,
    SemanticNodeId,
) {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let target = attach(&mut store, root, circle(0.0));
    let other = attach(&mut store, root, circle(4.0));
    let mut transaction = SemanticMutationTransaction::new();
    let signal = transaction.create_node(
        SemanticNodeCreation::native_input_signal(
            SemanticSignalValue::Scalar(0.0),
            SemanticNativeInputSource::Event(event),
        )
        .unwrap(),
    );
    transaction.scope_signal(root, signal);
    let signal = transaction
        .apply(&mut store)
        .unwrap()
        .resolve(signal)
        .unwrap();
    let gain = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let offset = store
        .insert_semantic_derived_signal(SemanticSignalExpr::Sub(
            Box::new(SemanticSignalExpr::signal(signal)),
            Box::new(SemanticSignalExpr::scalar(1.0)),
        ))
        .unwrap();
    let scaled = store
        .insert_semantic_derived_signal(SemanticSignalExpr::Mul(
            Box::new(SemanticSignalExpr::signal(offset)),
            Box::new(SemanticSignalExpr::signal(gain)),
        ))
        .unwrap();
    let squared = store
        .insert_semantic_derived_signal(SemanticSignalExpr::Mul(
            Box::new(SemanticSignalExpr::signal(scaled)),
            Box::new(SemanticSignalExpr::signal(scaled)),
        ))
        .unwrap();
    store
        .bind_semantic_signal(squared, other, SemanticObjectProperty::RotationZ)
        .unwrap();
    (store, root, target, other, gain)
}

#[test]
fn failed_release_rolls_back_selection_gesture_input_sequence_and_reactive_publication() {
    let (store, root, original, other, gain) =
        failing_edge_fixture(NativeEventSource::PointerUp { button: 0 });
    let mut session = session(&store, root);
    click(&mut session, 0, 0.0);
    session.set_reactive_input(gain, 1.0e20_f32).unwrap();
    submit(&mut session, 2, press(4.0, 100.0));
    let before = session.pointer_selection;
    let frame = session.frame().clone();
    let publication = session.publication_context();
    let (token, up) = input(&session, 3, release(4.0, 100.0));
    assert!(matches!(
        session.submit_native_pointer_input(&token, up),
        Err(ExecutionSessionInputError::Evaluation(_))
    ));
    assert_eq!(session.pointer_selection, before);
    assert_eq!(session.selected_pointer_target(), Some(original));
    assert_eq!(session.last_native_event_sequence, Some(2));
    assert_eq!(session.frame(), &frame);
    assert_eq!(session.publication_context(), publication);
    session.set_reactive_input(gain, 1.0_f32).unwrap();
    let retry = submit(&mut session, 3, release(4.0, 100.0));
    assert_eq!(retry.selection_click().unwrap().target(), Some(other));
}

#[test]
fn failed_press_does_not_retain_a_target_or_allow_a_later_release_to_click() {
    let (store, root, original, _, gain) =
        failing_edge_fixture(NativeEventSource::PointerDown { button: 0 });
    let mut session = session(&store, root);
    click(&mut session, 0, 0.0);
    session.set_reactive_input(gain, 1.0e20_f32).unwrap();
    let before = session.pointer_selection;
    let (token, down) = input(&session, 2, press(4.0, 100.0));
    assert!(matches!(
        session.submit_native_pointer_input(&token, down),
        Err(ExecutionSessionInputError::Evaluation(_))
    ));
    assert_eq!(session.pointer_selection, before);
    assert_eq!(session.last_native_event_sequence, Some(1));
    assert_eq!(
        submit(&mut session, 3, release(4.0, 100.0)).selection_click(),
        None
    );
    assert_eq!(session.selected_pointer_target(), Some(original));
}
