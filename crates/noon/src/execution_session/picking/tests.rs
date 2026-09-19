use std::sync::Arc;

use noon_core::{
    AnimationOptions, Color, NativeInputModifiers, NativePointerCancellation, NativePointerId,
    NativePointerInputKind, NativePointerPosition, RateFunction, SemanticMutationTransaction,
    SemanticNodeCreation, SemanticObjectProperty, SemanticObjectState, SemanticPaint,
    SemanticStore, SemanticVec3, StoredGeometry, Transform2D, VectorPath,
};

use super::*;

const POINTER: NativePointerId = NativePointerId {
    source: 4,
    pointer: 7,
};

fn circle() -> SemanticObjectState {
    SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 })
}

fn rectangle() -> SemanticObjectState {
    SemanticObjectState::new(StoredGeometry::Rectangle {
        size: Vec2::new(2.0, 2.0),
    })
}

fn attach(store: &mut SemanticStore, state: SemanticObjectState) -> SemanticNodeId {
    let node = store.insert_semantic_object(state);
    store.attach_to_scene(node).unwrap();
    node
}

fn session(store: &SemanticStore) -> ExecutionSession {
    let mut session = ExecutionSession::from_semantic_store(store).unwrap();
    session.configure_native_pointer_input(POINTER, 1).unwrap();
    session.take_frame_changes();
    session
}

fn record(
    session: &ExecutionSession,
    point: Vec2,
) -> (NativePointerInputToken, NativePointerInput) {
    let token = session.native_pointer_input_token().unwrap();
    let input = NativePointerInput::new(
        0,
        POINTER,
        token.context(),
        NativeInputModifiers {
            shift: true,
            ..Default::default()
        },
        NativePointerInputKind::Press {
            position: NativePointerPosition::new(point, Vec2::new(200.0, 100.0)).unwrap(),
            button: 0,
        },
    );
    (token, input)
}

fn query(session: &mut ExecutionSession, point: Vec2) -> PointerFillQuery {
    let (token, input) = record(session, point);
    session
        .pick_native_pointer_fill(&token, input, |_| true)
        .unwrap()
}

#[test]
fn bounds_only_false_positive_does_not_hide_the_precise_target_behind_it() {
    let mut store = SemanticStore::new();
    let bottom = attach(&mut store, rectangle());
    let top = attach(&mut store, circle());
    let mut session = session(&store);
    let corner = query(&mut session, Vec2::new(0.9, 0.9));
    assert_eq!(corner.spatial_stats().results, 2);
    assert_eq!(corner.precise_tests(), 2);
    assert_eq!(corner.outcome(), PointerFillOutcome::Hit(bottom));
    assert_eq!(
        query(&mut session, Vec2::ZERO).outcome(),
        PointerFillOutcome::Hit(top)
    );
    assert_eq!(
        query(&mut session, Vec2::new(2.0, 2.0)).outcome(),
        PointerFillOutcome::Miss
    );
}

#[test]
fn topmost_order_uses_z_order_and_candidate_local_eligibility() {
    let mut store = SemanticStore::new();
    let mut high = circle();
    high.set_z_index(3.0);
    let top = attach(&mut store, high);
    let last_inserted = attach(&mut store, rectangle());
    let mut session = session(&store);
    assert_eq!(
        query(&mut session, Vec2::ZERO).outcome(),
        PointerFillOutcome::Hit(top)
    );
    let (token, input) = record(&session, Vec2::ZERO);
    let mut visited = Vec::new();
    let result = session
        .pick_native_pointer_fill(&token, input, |node| {
            visited.push(node);
            node != top
        })
        .unwrap();
    assert_eq!(visited, vec![top, last_inserted]);
    assert_eq!(result.outcome(), PointerFillOutcome::Hit(last_inserted));
    assert_eq!(result.precise_tests(), 1);
}

#[test]
fn reflected_rotated_nonuniform_fills_use_inverse_effective_transforms() {
    let mut store = SemanticStore::new();
    let mut state = circle();
    state.transform.translation = SemanticVec3::new(4.0, -3.0, 0.0);
    state.transform.scale = SemanticVec3::new(-3.0, 0.5, 1.0);
    state.transform.rotation_z = 0.7;
    let node = attach(&mut store, state);
    let mut session = session(&store);
    let transform = session.frame().render_transform(0);
    let inside = transform.transform_point(Vec2::new(0.4, 0.2));
    let outside = transform.transform_point(Vec2::new(0.85, 0.85));
    assert_eq!(
        query(&mut session, inside).outcome(),
        PointerFillOutcome::Hit(node)
    );
    let miss = query(&mut session, outside);
    assert_eq!(
        miss.spatial_stats().results,
        1,
        "point is inside the conservative rotated box"
    );
    assert_eq!(miss.outcome(), PointerFillOutcome::Miss);
}

#[test]
fn rotated_rectangle_rejects_its_world_axis_aligned_box_corner() {
    let mut store = SemanticStore::new();
    let mut state = rectangle();
    state.transform.rotation_z = std::f64::consts::FRAC_PI_4;
    attach(&mut store, state);
    let mut session = session(&store);
    let result = query(&mut session, Vec2::new(1.2, 1.2));
    assert_eq!(result.spatial_stats().results, 1);
    assert_eq!(result.outcome(), PointerFillOutcome::Miss);
}

#[test]
fn transparent_and_stroke_only_shapes_do_not_claim_fill_hits() {
    let mut store = SemanticStore::new();
    let bottom = attach(&mut store, rectangle());
    for mode in 0..4 {
        let mut state = circle();
        match mode {
            0 => state.style.fill_opacity = 0.0,
            1 => state.style.object_opacity = 0.0,
            2 => state.style.fill = Some(SemanticPaint::Solid(Color::TRANSPARENT)),
            _ => {
                state.style.fill = None;
                state.style.stroke = Some(SemanticPaint::Solid(Color::WHITE));
                state.style.stroke_width = 0.5;
            }
        }
        attach(&mut store, state);
    }
    let mut session = session(&store);
    let hit = query(&mut session, Vec2::ZERO);
    assert_eq!(hit.outcome(), PointerFillOutcome::Hit(bottom));
    assert_eq!(hit.precise_tests(), 1);
    assert_eq!(
        query(&mut session, Vec2::new(1.2, 0.0)).outcome(),
        PointerFillOutcome::Miss,
        "the stroke band is explicitly outside this fill policy"
    );
}

#[test]
fn unsupported_eligible_content_is_not_a_box_hit_or_a_silent_pass_through() {
    let mut store = SemanticStore::new();
    let bottom = attach(&mut store, rectangle());
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
        SemanticObjectState::new(StoredGeometry::Resource(path)),
    );
    let mut session = session(&store);
    assert_eq!(
        query(&mut session, Vec2::ZERO).outcome(),
        PointerFillOutcome::Unsupported {
            target: top,
            reason: PointerFillUnsupported::Content,
        }
    );
    let (token, input) = record(&session, Vec2::ZERO);
    assert_eq!(
        session
            .pick_native_pointer_fill(&token, input, |node| node == bottom)
            .unwrap()
            .outcome(),
        PointerFillOutcome::Hit(bottom)
    );
}

#[test]
fn query_keeps_original_occurrence_and_does_not_acknowledge_or_publish_input() {
    let mut store = SemanticStore::new();
    let node = attach(&mut store, circle());
    let mut session = session(&store);
    let (token, input) = record(&session, Vec2::new(0.25, 0.5));
    let before = session.publication_context();
    let frame = session.frame().clone();
    let result = session
        .pick_native_pointer_fill(&token, input, |_| true)
        .unwrap();
    assert_eq!(result.input(), input);
    assert_eq!(result.publication(), before);
    assert_eq!(result.outcome(), PointerFillOutcome::Hit(node));
    assert_eq!(session.last_native_event_sequence, None);
    assert_eq!(session.frame(), &frame);
    assert_eq!(session.publication_context(), before);
    assert!(session.take_frame_changes().is_empty());
    assert!(session.wake_state().is_quiescent());
    session.submit_native_pointer_input(&token, input).unwrap();
    assert_eq!(session.last_native_event_sequence, Some(0));
    assert!(matches!(
        session.pick_native_pointer_fill(&token, input, |_| true),
        Err(ExecutionSessionInputError::NativeEventOutOfOrder { .. })
    ));
}

#[test]
fn stale_publication_foreign_runtime_and_replaced_view_are_rejected_before_picking() {
    let mut store = SemanticStore::new();
    attach(&mut store, circle());
    let mut original = session(&store);
    let (token, input) = record(&original, Vec2::ZERO);
    let mut cloned = original.clone();
    assert_eq!(cloned.publication_context(), original.publication_context());
    assert!(matches!(
        cloned.pick_native_pointer_fill(&token, input, |_| panic!("must not query")),
        Err(ExecutionSessionInputError::ForeignPointerRuntime)
    ));
    original.advance_to(1.0).unwrap();
    assert!(matches!(
        original.pick_native_pointer_fill(&token, input, |_| panic!("must not query")),
        Err(ExecutionSessionInputError::StalePointerPublication { .. })
    ));
    let (token, input) = record(&original, Vec2::ZERO);
    original.configure_native_pointer_input(POINTER, 2).unwrap();
    assert!(matches!(
        original.pick_native_pointer_fill(&token, input, |_| panic!("must not query")),
        Err(ExecutionSessionInputError::StalePointerBinding)
    ));
}

#[test]
fn cancellation_has_no_pick_position_and_cannot_bypass_a_callback_barrier() {
    let mut store = SemanticStore::new();
    let node = attach(&mut store, circle());
    let mut session = session(&store);
    let token = session.native_pointer_input_token().unwrap();
    let input = NativePointerInput::new(
        0,
        POINTER,
        token.context(),
        Default::default(),
        NativePointerInputKind::Cancel(NativePointerCancellation::CaptureLost),
    );
    let result = session
        .pick_native_pointer_fill(&token, input, |_| panic!("no position"))
        .unwrap();
    assert_eq!(result.outcome(), PointerFillOutcome::NoPosition);
    assert_eq!(result.precise_tests(), 0);
    assert_eq!(result.spatial_stats(), SpatialQueryStats::default());
    assert_eq!(session.last_native_event_sequence, None);
    let overlay = session.begin_required_callback_phase(0.0, [node]).unwrap();
    assert!(matches!(
        session.pick_native_pointer_fill(&token, input, |_| true),
        Err(ExecutionSessionInputError::RequiredCallbackPending)
    ));
    session
        .commit_required_callback_phase(overlay.finish())
        .unwrap();
}

#[test]
fn animated_object_is_picked_at_its_current_effective_position_only() {
    let mut store = SemanticStore::new();
    let node = attach(&mut store, circle());
    let mut target = circle();
    target.transform.translation = SemanticVec3::new(8.0, 0.0, 0.0);
    let target = store.insert_semantic_object(target);
    let animation = store
        .insert_semantic_transform_animation(node, target, AnimationOptions::new())
        .unwrap();
    let mut session = session(&store);
    session
        .activate_animation_segment(
            &store,
            animation,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    session.advance_to(1.0).unwrap();
    let at = session.publication_context();
    assert_eq!(
        query(&mut session, Vec2::new(4.0, 0.0)).outcome(),
        PointerFillOutcome::Hit(node)
    );
    assert_eq!(
        query(&mut session, Vec2::ZERO).outcome(),
        PointerFillOutcome::Miss
    );
    assert_eq!(session.frame().time, 1.0);
    assert_eq!(session.publication_context(), at);
}

#[test]
fn removed_and_reused_semantic_nodes_do_not_resolve_as_the_old_target() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let old = store.insert_semantic_object(circle());
    store.add_semantic_family_member(root, old).unwrap();
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.configure_native_pointer_input(POINTER, 1).unwrap();
    assert_eq!(
        query(&mut session, Vec2::ZERO).outcome(),
        PointerFillOutcome::Hit(old)
    );
    let old_key = session.execution_object_id(old).unwrap();
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_node(old);
    session
        .apply_semantic_transaction(&mut store, remove)
        .unwrap();
    assert_eq!(session.execution_index.semantic_object_id(old_key), None);
    assert_eq!(
        query(&mut session, Vec2::ZERO).outcome(),
        PointerFillOutcome::Miss
    );
    let mut add = SemanticMutationTransaction::new();
    let pending = add.create_node(SemanticNodeCreation::object(rectangle()));
    add.add_member(root, pending);
    let created = session
        .apply_semantic_transaction(&mut store, add)
        .unwrap()
        .resolve(pending)
        .unwrap();
    assert_ne!(created, old);
    assert_eq!(
        query(&mut session, Vec2::ZERO).outcome(),
        PointerFillOutcome::Hit(created)
    );
    assert_eq!(session.execution_index.semantic_object_id(old_key), None);
}

#[test]
fn clean_and_locally_changed_queries_do_not_scan_ten_thousand_unrelated_objects() {
    let mut store = SemanticStore::new();
    let moving = store
        .insert_semantic_input_signal(SemanticVec3::ZERO)
        .unwrap();
    let target = attach(&mut store, circle());
    store
        .bind_semantic_signal(moving, target, SemanticObjectProperty::Translation)
        .unwrap();
    for i in 0..10_000 {
        let mut state = circle();
        state.transform.translation = SemanticVec3::new(20.0 + f64::from(i) * 6.0, 0.0, 0.0);
        attach(&mut store, state);
    }
    let mut session = session(&store);
    let first = query(&mut session, Vec2::ZERO);
    assert_eq!(first.outcome(), PointerFillOutcome::Hit(target));
    assert_eq!(first.precise_tests(), 1);
    assert_eq!(first.spatial_stats().candidates_tested, 1);
    assert_eq!(first.spatial_stats().full_scan_fallbacks, 0);
    query(&mut session, Vec2::ZERO);
    assert_eq!(session.last_spatial_update_stats().full_rebuilds, 0);
    assert_eq!(session.last_spatial_update_stats().leaves_upserted, 0);
    session
        .set_reactive_input(moving, Vec2::new(5.0, 0.0))
        .unwrap();
    assert_eq!(
        query(&mut session, Vec2::new(5.0, 0.0)).outcome(),
        PointerFillOutcome::Hit(target)
    );
    assert_eq!(session.last_spatial_update_stats().full_rebuilds, 0);
    assert_eq!(session.last_spatial_update_stats().leaves_upserted, 1);
    assert_eq!(
        session.take_frame_changes().object_indices(),
        &[0],
        "picking must not consume renderer dirtiness"
    );
}

#[test]
fn effective_render_overrides_and_partial_geometry_have_explicit_policy() {
    let mut store = SemanticStore::new();
    attach(&mut store, circle());
    let session = session(&store);
    let mut frame = session.frame().clone();
    frame.render_geometries[0] = Some(Arc::new(GeometryRef::Rectangle {
        size: Vec2::new(2.0, 2.0),
    }));
    frame.render_transforms[0] = Some(Transform2D {
        translation: Vec2::new(4.0, 0.0),
        ..Default::default()
    });
    assert_eq!(
        analytic_fill_contains(&frame, 0, Vec2::new(4.9, 0.9)),
        Ok(true)
    );
    assert_eq!(analytic_fill_contains(&frame, 0, Vec2::ZERO), Ok(false));
    frame.reveals[0] = 0.5;
    assert_eq!(
        analytic_fill_contains(&frame, 0, Vec2::new(4.0, 0.0)),
        Err(PointerFillUnsupported::PartialGeometry)
    );
    frame.reveals[0] = 1.0;
    frame.morphs[0] = 0.5;
    assert_eq!(
        analytic_fill_contains(&frame, 0, Vec2::new(4.0, 0.0)),
        Err(PointerFillUnsupported::PartialGeometry)
    );
}

#[test]
fn nominal_contour_is_inclusive_and_zero_scale_is_not_inverted() {
    let mut store = SemanticStore::new();
    let node = attach(&mut store, circle());
    let mut session = session(&store);
    assert_eq!(
        query(&mut session, Vec2::new(1.0, 0.0)).outcome(),
        PointerFillOutcome::Hit(node)
    );
    let mut frame = session.frame().clone();
    frame.render_transforms[0] = Some(Transform2D {
        scale: Vec2::new(0.0, 1.0),
        ..Default::default()
    });
    assert_eq!(
        analytic_fill_contains(&frame, 0, Vec2::ZERO),
        Err(PointerFillUnsupported::DegenerateGeometry)
    );
}
