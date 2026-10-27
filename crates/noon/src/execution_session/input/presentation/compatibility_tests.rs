//! Collected-point compatibility through the ordinary shared Rust input lane.
//! These fixtures use authored tracks and semantic transactions, never a second
//! picker, a rewritten occurrence context, or direct runtime frame mutations.

use super::*;
use crate::integration::PointerFillOutcome;
use noon_core::{
    Color, CompositionTimeMap, NativeInputModifiers, NativePointerId, NativePointerInput,
    NativePointerInputKind, NativePointerPosition, PlaybackControl, RateFunction,
    SemanticAnimationCompositionKind, SemanticClickAction, SemanticMutationTransaction,
    SemanticNodeId, SemanticObjectProperty, SemanticObjectRole, SemanticObjectState,
    SemanticObjectTrackProperty, SemanticObjectTrackValues, SemanticStore,
    SemanticTransactionNodeRef, SemanticVec3, StoredGeometry, TrackTiming,
};
use noon_runtime::frame_object_conservative_bounds;

const POINTER: NativePointerId = NativePointerId {
    source: 91,
    pointer: 7,
};
const VIEW: u64 = 3;
const SIZE: Vec2 = Vec2::new(800.0, 400.0);
const CENTER: Vec2 = Vec2::new(400.0, 200.0);

struct Fixture {
    store: SemanticStore,
    nodes: Vec<SemanticNodeId>,
    session: ExecutionSession,
}

fn circle(x: f64) -> SemanticObjectState {
    let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.75 });
    state.transform.translation = SemanticVec3::new(x, 0.0, 0.0);
    state
}

fn button() -> SemanticObjectState {
    let mut state = circle(0.0);
    state.set_click_action(Some(SemanticClickAction::Playback(
        PlaybackControl::NextSection,
    )));
    state
}

fn tracked_fixture(
    objects: Vec<SemanticObjectState>,
    animated: usize,
    property: SemanticObjectTrackProperty,
    values: SemanticObjectTrackValues<SemanticTransactionNodeRef>,
    rate: RateFunction,
) -> Fixture {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let nodes: Vec<_> = objects
        .into_iter()
        .map(|state| {
            let node = store.insert_semantic_object(state);
            store.add_semantic_family_member(root, node).unwrap();
            node
        })
        .collect();
    let mut transaction = SemanticMutationTransaction::new();
    let track = transaction.create_object_property_track(
        nodes[animated],
        property,
        values,
        TrackTiming::new(0.0, 2.0, rate),
        CompositionTimeMap::identity(),
    );
    let animation = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [track],
        Default::default(),
    );
    let committed = transaction.apply(&mut store).unwrap();
    store.attach_to_scene(root).unwrap();
    let mut session = ExecutionSession::from_semantic_root_with_animation_root(
        &store,
        root,
        committed.resolve(animation).unwrap(),
    )
    .unwrap();
    session
        .configure_native_pointer_input(POINTER, VIEW)
        .unwrap();
    session.enable_pointer_fill_selection(4.0).unwrap();
    session.take_frame_changes();
    Fixture {
        store,
        nodes,
        session,
    }
}

fn position_track(from: f64, to: f64) -> SemanticObjectTrackValues<SemanticTransactionNodeRef> {
    SemanticObjectTrackValues::Vec3 {
        from: SemanticVec3::new(from, 0.0, 0.0),
        to: SemanticVec3::new(to, 0.0, 0.0),
    }
}

fn distant_fixture() -> Fixture {
    tracked_fixture(
        vec![button(), circle(8.0)],
        1,
        SemanticObjectTrackProperty::Position,
        position_track(8.0, 12.0),
        RateFunction::Linear,
    )
}

fn view(session: &ExecutionSession) -> PointerFrameView {
    session.inspection_pointer_view(VIEW, SIZE).unwrap()
}

fn capture(session: &mut ExecutionSession) -> PointerFrameSnapshot {
    let current_view = view(session);
    session.capture_pointer_frame(current_view).unwrap()
}

fn record(
    token: &NativePointerInputToken,
    sequence: u64,
    kind: NativePointerInputKind,
) -> NativePointerInput {
    NativePointerInput::new(
        sequence,
        token.pointer(),
        token.context(),
        NativeInputModifiers::default(),
        kind,
    )
}

fn edge(frame: &PointerFrameSnapshot, surface: Vec2, pressed: bool) -> NativePointerInputKind {
    let position = frame.position(surface).unwrap();
    if pressed {
        NativePointerInputKind::Press {
            position,
            button: 0,
        }
    } else {
        NativePointerInputKind::Release {
            position,
            button: 0,
        }
    }
}

fn assert_position_rejected(
    frame: &PointerFrameSnapshot,
    session: &mut ExecutionSession,
    current_view: PointerFrameView,
    surface: Vec2,
) {
    let publication = session.publication_context();
    let last_sequence = session.last_native_event_sequence;
    let time = session.frame().time;
    assert!(frame
        .validate_position(session, current_view, surface)
        .is_err());
    assert!(frame
        .input_token_at(session, current_view, surface)
        .is_err());
    assert_eq!(session.publication_context(), publication);
    assert_eq!(session.frame().time, time);
    assert_eq!(session.last_native_event_sequence, last_sequence);
}

fn assert_current_position_rejected(
    frame: &PointerFrameSnapshot,
    session: &mut ExecutionSession,
    surface: Vec2,
) {
    let current_view = view(session);
    assert_position_rejected(frame, session, current_view, surface);
}

fn current_pick(session: &mut ExecutionSession) -> PointerFillOutcome {
    let token = session.native_pointer_input_token().unwrap();
    let occurrence = record(
        &token,
        session.last_native_event_sequence.map_or(0, |n| n + 1),
        NativePointerInputKind::Press {
            position: NativePointerPosition::new(Vec2::ZERO, CENTER).unwrap(),
            button: 0,
        },
    );
    session
        .pick_native_pointer_fill(&token, occurrence, |_| true)
        .unwrap()
        .outcome()
}

#[test]
fn delayed_click_keeps_collected_contexts_while_a_distant_region_animates() {
    // Both a queued down/up from one display and a held press whose release was
    // collected from a later display use each edge's own original context.
    for release_from_later_display in [false, true] {
        let mut fixture = distant_fixture();
        let session = &mut fixture.session;
        let down_frame = capture(session);
        let authored_revision = fixture.store.scene_revision();
        session.advance_to(0.5).unwrap();
        let before_down = session.publication_context();
        assert_ne!(
            before_down.frame_epoch(),
            down_frame.publication().frame_epoch()
        );
        let current_view = view(session);
        down_frame
            .validate_position(session, current_view, CENTER)
            .unwrap();
        let down_token = down_frame
            .input_token_at(session, current_view, CENTER)
            .unwrap();
        assert_eq!(down_token.context().publication, down_frame.publication());
        let down = session
            .submit_native_pointer_input(
                &mut fixture.store,
                &down_token,
                record(&down_token, 10, edge(&down_frame, CENTER, true)),
            )
            .unwrap();
        assert_eq!(down.input().context().publication, down_frame.publication());
        assert_eq!(
            down.input().position(),
            Some(down_frame.position(CENTER).unwrap())
        );
        assert_eq!(down.previous_publication(), before_down);
        assert_eq!(down.publication(), before_down);
        assert_eq!(down.playback_control(), None);
        let up_frame = if release_from_later_display {
            capture(session)
        } else {
            down_frame.clone()
        };
        session.advance_to(1.0).unwrap();
        let before_up = session.publication_context();
        let current_view = view(session);
        let up_token = up_frame
            .input_token_at(session, current_view, CENTER)
            .unwrap();
        let up = session
            .submit_native_pointer_input(
                &mut fixture.store,
                &up_token,
                record(&up_token, 11, edge(&up_frame, CENTER, false)),
            )
            .unwrap();
        assert_eq!(up.input().context().publication, up_frame.publication());
        assert_eq!(up.previous_publication(), before_up);
        assert_eq!(up.publication(), before_up);
        let click = up.selection_click().unwrap();
        assert_eq!(click.target(), Some(fixture.nodes[0]));
        assert_eq!(
            click.press().context().publication,
            down_frame.publication()
        );
        assert_eq!(
            click.release().context().publication,
            up_frame.publication()
        );
        assert_eq!(up.playback_control(), Some(PlaybackControl::NextSection));
        assert_eq!(session.frame().time, 1.0);
        assert_eq!(fixture.store.scene_revision(), authored_revision);
        assert_eq!(session.last_native_event_sequence, Some(11));
        assert_eq!(up.selection_query().unwrap().precise_tests(), 1);
    }
}

#[test]
fn movement_of_the_collected_target_rejects_the_old_point() {
    let mut fixture = tracked_fixture(
        vec![button()],
        0,
        SemanticObjectTrackProperty::Position,
        position_track(0.0, 4.0),
        RateFunction::Linear,
    );
    let session = &mut fixture.session;
    let frame = capture(session);
    session.advance_to(1.0).unwrap();
    assert_eq!(current_pick(session), PointerFillOutcome::Miss);
    assert_current_position_rejected(&frame, session, CENTER);
}

#[test]
fn a_new_occluder_at_the_collected_point_cannot_dispatch_the_old_target() {
    let mut fixture = tracked_fixture(
        vec![button(), circle(4.0)],
        1,
        SemanticObjectTrackProperty::Position,
        position_track(4.0, 0.0),
        RateFunction::Linear,
    );
    let session = &mut fixture.session;
    let frame = capture(session);
    assert_eq!(
        current_pick(session),
        PointerFillOutcome::Hit(fixture.nodes[0])
    );
    session.advance_to(2.0).unwrap();
    assert_eq!(
        current_pick(session),
        PointerFillOutcome::Hit(fixture.nodes[1])
    );
    assert_current_position_rejected(&frame, session, CENTER);
}

#[test]
fn fill_change_is_damage_even_when_bounds_and_transform_are_identical() {
    let mut fixture = tracked_fixture(
        vec![button()],
        0,
        SemanticObjectTrackProperty::Fill,
        SemanticObjectTrackValues::Color {
            from: Some(Color::WHITE),
            to: Some(Color::rgba(1.0, 1.0, 1.0, 0.0)),
        },
        RateFunction::Linear,
    );
    let session = &mut fixture.session;
    let frame = capture(session);
    let before = session.frame().objects[0].clone();
    let before_bounds = frame_object_conservative_bounds(session.frame(), 0).unwrap();
    session.advance_to(2.0).unwrap();
    let after = &session.frame().objects[0];
    assert_eq!(
        frame_object_conservative_bounds(session.frame(), 0),
        Some(before_bounds)
    );
    assert_eq!(after.transform, before.transform);
    assert_ne!(after.style.fill, before.style.fill);
    assert_current_position_rejected(&frame, session, CENTER);
}

#[test]
fn nonspatial_fill_damage_survives_renderer_dirtiness_consumption() {
    let mut fixture = tracked_fixture(
        vec![button()],
        0,
        SemanticObjectTrackProperty::Fill,
        SemanticObjectTrackValues::Color {
            from: Some(Color::WHITE),
            to: Some(Color::rgba(1.0, 1.0, 1.0, 0.0)),
        },
        RateFunction::Linear,
    );
    let session = &mut fixture.session;
    let frame = capture(session);
    session.advance_to(1.0).unwrap();
    // Renderer publication can consume its own dirty list before an input
    // arrives. That must not erase the independent picking-damage journal.
    assert!(session.take_frame_changes().has_stable_changes());
    assert_current_position_rejected(&frame, session, CENTER);
}

#[test]
fn distant_fill_change_keeps_collected_point_compatible_without_spatial_reindex() {
    let mut fixture = tracked_fixture(
        vec![button(), circle(8.0)],
        1,
        SemanticObjectTrackProperty::Fill,
        SemanticObjectTrackValues::Color {
            from: Some(Color::WHITE),
            to: Some(Color::rgba(1.0, 1.0, 1.0, 0.0)),
        },
        RateFunction::Linear,
    );
    let session = &mut fixture.session;
    let frame = capture(session);
    session.advance_to(1.0).unwrap();
    let current_view = view(session);
    frame
        .validate_position(session, current_view, CENTER)
        .unwrap();
    assert!(frame.input_token_at(session, current_view, CENTER).is_ok());
    assert_eq!(session.last_spatial_update_stats().leaves_upserted, 0);
}

#[test]
fn reveal_change_is_damage_even_when_bounds_and_transform_are_identical() {
    let mut fixture = tracked_fixture(
        vec![button()],
        0,
        SemanticObjectTrackProperty::Reveal,
        SemanticObjectTrackValues::Scalar { from: 1.0, to: 0.0 },
        RateFunction::Linear,
    );
    let session = &mut fixture.session;
    let frame = capture(session);
    let before = session.frame().objects[0].clone();
    let before_bounds = frame_object_conservative_bounds(session.frame(), 0).unwrap();
    let previous_reveal = session.frame().reveals[0];
    session.advance_to(1.0).unwrap();
    let after = &session.frame().objects[0];
    assert_eq!(
        frame_object_conservative_bounds(session.frame(), 0),
        Some(before_bounds)
    );
    assert_eq!(after.transform, before.transform);
    assert_ne!(session.frame().reveals[0], previous_reveal);
    assert_current_position_rejected(&frame, session, CENTER);
}

#[test]
fn captured_intermediate_occlusion_cannot_disappear_when_the_endpoint_returns() {
    let mut fixture = tracked_fixture(
        vec![button(), circle(4.0)],
        1,
        SemanticObjectTrackProperty::Position,
        position_track(4.0, 0.0),
        RateFunction::ThereAndBack,
    );
    let session = &mut fixture.session;
    let first = capture(session);
    let initial = session.frame().objects[1].transform;
    session.advance_to(1.0).unwrap();
    assert_eq!(session.frame().objects[1].transform.translation, Vec2::ZERO);
    // A real publication capture must preserve this damage even if no picker
    // consumes the intermediate spatial state before the next advance.
    let middle = capture(session);
    assert_ne!(middle.publication(), first.publication());
    session.advance_to(2.0).unwrap();
    assert_eq!(session.frame().objects[1].transform, initial);
    assert_current_position_rejected(&first, session, CENTER);
    assert_current_position_rejected(&middle, session, CENTER);
    assert_eq!(
        current_pick(session),
        PointerFillOutcome::Hit(fixture.nodes[0])
    );
}

#[test]
fn a_point_qualified_token_cannot_be_substituted_at_picking_or_commit() {
    let mut fixture = distant_fixture();
    let session = &mut fixture.session;
    let frame = capture(session);
    session.advance_to(0.5).unwrap();
    let current_view = view(session);
    let token = frame.input_token_at(session, current_view, CENTER).unwrap();
    let publication = session.publication_context();
    let shifted = frame.position(CENTER + Vec2::new(1.0, 0.0)).unwrap();
    let forged_mapping = NativePointerPosition::new(Vec2::new(0.02, 0.0), CENTER).unwrap();
    for position in [shifted, forged_mapping] {
        let occurrence = record(
            &token,
            20,
            NativePointerInputKind::Press {
                position,
                button: 0,
            },
        );
        assert!(session
            .pick_native_pointer_fill(&token, occurrence, |_| true)
            .is_err());
        assert!(session
            .submit_native_pointer_input(&mut fixture.store, &token, occurrence)
            .is_err());
        assert_eq!(session.publication_context(), publication);
        assert_eq!(session.last_native_event_sequence, None);
    }
    // Failed substitution cannot consume the sequence or poison a valid proof.
    let receipt = session
        .submit_native_pointer_input(
            &mut fixture.store,
            &token,
            record(&token, 20, edge(&frame, CENTER, true)),
        )
        .unwrap();
    assert_eq!(receipt.input().context().publication, frame.publication());
    assert_eq!(session.last_native_event_sequence, Some(20));
}

#[test]
fn advancing_after_point_validation_requires_a_new_proof_even_for_distant_damage() {
    let mut fixture = distant_fixture();
    let session = &mut fixture.session;
    let frame = capture(session);
    session.advance_to(0.5).unwrap();
    let current_view = view(session);
    let old_token = frame.input_token_at(session, current_view, CENTER).unwrap();
    session.advance_to(1.0).unwrap();
    let occurrence = record(&old_token, 30, edge(&frame, CENTER, true));
    assert!(session
        .pick_native_pointer_fill(&old_token, occurrence, |_| true)
        .is_err());
    assert!(session
        .submit_native_pointer_input(&mut fixture.store, &old_token, occurrence)
        .is_err());
    assert_eq!(session.last_native_event_sequence, None);
    let fresh_proof = frame.input_token_at(session, current_view, CENTER).unwrap();
    assert_eq!(fresh_proof.context().publication, frame.publication());
    let receipt = session
        .submit_native_pointer_input(
            &mut fixture.store,
            &fresh_proof,
            record(&fresh_proof, 30, edge(&frame, CENTER, true)),
        )
        .unwrap();
    assert_eq!(receipt.input().context().publication, frame.publication());
}

#[test]
fn changed_view_revision_size_or_camera_cannot_reuse_a_point_proof() {
    let mut fixture = distant_fixture();
    let session = &mut fixture.session;
    let frame = capture(session);
    session.advance_to(0.5).unwrap();
    for changed in [
        PointerFrameView::new(VIEW + 1, SIZE, Camera2DState::default()).unwrap(),
        PointerFrameView::new(VIEW, SIZE * 2.0, Camera2DState::default()).unwrap(),
        PointerFrameView::new(
            VIEW,
            SIZE,
            Camera2DState {
                center: Vec2::new(1.0, 0.0),
                height: 8.0,
            },
        )
        .unwrap(),
    ] {
        assert_position_rejected(&frame, session, changed, CENTER);
    }
}

#[test]
fn an_animated_execution_camera_invalidates_the_collected_mapping() {
    let mut camera = SemanticObjectState::new(StoredGeometry::Rectangle {
        size: Vec2::new(16.0, 8.0),
    });
    camera.set_role(SemanticObjectRole::Camera2D);
    let mut fixture = tracked_fixture(
        vec![button(), camera],
        1,
        SemanticObjectTrackProperty::Position,
        position_track(0.0, 2.0),
        RateFunction::Linear,
    );
    let session = &mut fixture.session;
    let frame = capture(session);
    session.advance_to(1.0).unwrap();
    assert_ne!(session.inspection_camera().unwrap(), frame.view().camera());
    assert_position_rejected(&frame, session, frame.view(), CENTER);
    assert_current_position_rejected(&frame, session, CENTER);
}

#[test]
fn a_captured_camera_excursion_retires_points_outside_its_local_bounds() {
    let mut camera = SemanticObjectState::new(StoredGeometry::Rectangle {
        size: Vec2::new(16.0, 8.0),
    });
    camera.set_role(SemanticObjectRole::Camera2D);
    let mut fixture = tracked_fixture(
        vec![button(), camera],
        1,
        SemanticObjectTrackProperty::Position,
        position_track(0.0, 2.0),
        RateFunction::ThereAndBack,
    );
    let session = &mut fixture.session;
    let first = capture(session);
    let outside = Vec2::new(SIZE.x * 2.0, CENTER.y);
    let mapped = first.position(outside).unwrap();
    assert!(outside.x > first.view().viewport().x);
    assert!(
        mapped.scene().x
            > frame_object_conservative_bounds(session.frame(), 1)
                .unwrap()
                .max
                .x
    );
    first
        .validate_position(session, first.view(), outside)
        .unwrap();

    session.advance_to(1.0).unwrap();
    // Capturing the actual intermediate publication checkpoints the camera's
    // dirty row without an extra pick or a fabricated runtime frame.
    let middle = capture(session);
    assert_ne!(middle.view().camera(), first.view().camera());
    assert!(
        mapped.scene().x
            > frame_object_conservative_bounds(session.frame(), 1)
                .unwrap()
                .max
                .x
    );

    session.advance_to(2.0).unwrap();
    assert_eq!(view(session), first.view());
    assert_eq!(
        session.publication_context().scene_revision(),
        first.publication().scene_revision()
    );
    assert_eq!(
        session.publication_context().execution_revision(),
        first.publication().execution_revision()
    );
    assert_current_position_rejected(&first, session, outside);
    let restored = capture(session);
    restored
        .validate_position(session, restored.view(), outside)
        .unwrap();
}

#[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
#[test]
fn authored_glyph_progress_and_retirement_retire_off_row_point_proofs() {
    let mut scene = crate::Scene::new();
    let text = scene.text(crate::Text::new("Write")).unwrap();
    let mut session = scene.execution_session().unwrap();
    scene
        .live(&mut session)
        .declare_and_activate_text_write(
            &text,
            false,
            crate::AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    session
        .configure_native_pointer_input(POINTER, VIEW)
        .unwrap();
    session.enable_pointer_fill_selection(4.0).unwrap();
    session.advance_to(0.5).unwrap();
    assert_eq!(session.active_family_animation_indices().len(), 1);
    let first = capture(&mut session);
    let outside = Vec2::new(SIZE.x * 2.0, CENTER.y);
    let bounds = frame_object_conservative_bounds(session.frame(), 0).unwrap();
    assert!(first.position(outside).unwrap().scene().x > bounds.max.x);
    let first_progress = session.frame().family_animations[0]
        .unwrap()
        .overall_progress;
    first
        .input_token_at(&mut session, first.view(), outside)
        .unwrap();

    session.advance_to(1.0).unwrap();
    assert_ne!(
        session.frame().family_animations[0]
            .unwrap()
            .overall_progress,
        first_progress
    );
    assert_eq!(
        frame_object_conservative_bounds(session.frame(), 0),
        Some(bounds)
    );
    assert_current_position_rejected(&first, &mut session, outside);
    let changed = capture(&mut session);
    changed
        .input_token_at(&mut session, changed.view(), outside)
        .unwrap();

    session.advance_to(2.0).unwrap();
    assert_eq!(session.active_family_animation_indices().len(), 1);
    let endpoint = capture(&mut session);
    endpoint
        .input_token_at(&mut session, endpoint.view(), outside)
        .unwrap();
    // The exact endpoint remains a real glyph presentation; ordinary advance
    // beyond it retires the channel without an authored mutation or new plan.
    session.advance_to(2.25).unwrap();
    assert!(session.active_family_animation_indices().is_empty());
    assert_eq!(session.frame().family_animations[0], None);
    assert_eq!(
        frame_object_conservative_bounds(session.frame(), 0),
        Some(bounds)
    );
    assert_eq!(view(&session), endpoint.view());
    assert_eq!(
        session.publication_context().scene_revision(),
        first.publication().scene_revision()
    );
    assert_eq!(
        session.publication_context().execution_revision(),
        first.publication().execution_revision()
    );
    assert_current_position_rejected(&endpoint, &mut session, outside);
    assert_current_position_rejected(&first, &mut session, outside);
    let retired = capture(&mut session);
    retired
        .input_token_at(&mut session, retired.view(), outside)
        .unwrap();
}

#[test]
fn restoring_the_inspection_camera_does_not_revive_its_old_point_proof() {
    let mut fixture = distant_fixture();
    let session = &mut fixture.session;
    let frame = capture(session);
    assert!(session
        .zoom_inspection_view(&frame, frame.view(), CENTER, 0.5)
        .unwrap());
    let zoomed = capture(session);
    assert!(session
        .zoom_inspection_view(&zoomed, zoomed.view(), CENTER, 2.0)
        .unwrap());
    assert_eq!(view(session), frame.view());
    assert_ne!(
        session.inspection_view_revision(),
        frame.inspection_revision()
    );
    assert_current_position_rejected(&frame, session, CENTER);
}

#[test]
fn a_semantic_revision_change_is_a_hard_barrier_even_away_from_the_point() {
    let mut fixture = tracked_fixture(
        vec![circle(0.0), circle(8.0)],
        1,
        SemanticObjectTrackProperty::Position,
        position_track(8.0, 12.0),
        RateFunction::Linear,
    );
    let session = &mut fixture.session;
    let frame = capture(session);
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(fixture.nodes[1], SemanticObjectProperty::FillOpacity, 0.5);
    session
        .apply_semantic_transaction(&mut fixture.store, transaction)
        .unwrap();
    assert_ne!(
        session.publication_context().scene_revision(),
        frame.publication().scene_revision()
    );
    assert_current_position_rejected(&frame, session, CENTER);
}

#[test]
fn changing_selection_policy_retires_the_collected_proof() {
    let mut fixture = distant_fixture();
    let session = &mut fixture.session;
    let frame = capture(session);
    session.advance_to(0.5).unwrap();
    let current_view = view(session);
    let token = frame.input_token_at(session, current_view, CENTER).unwrap();
    session.enable_pointer_fill_selection(8.0).unwrap();
    assert_position_rejected(&frame, session, current_view, CENTER);
    assert!(session
        .submit_native_pointer_input(
            &mut fixture.store,
            &token,
            record(&token, 40, edge(&frame, CENTER, true))
        )
        .is_err());
    assert_eq!(session.last_native_event_sequence, None);
}

#[test]
fn a_point_proof_cannot_cross_a_runtime_or_pointer_owner_replacement() {
    let mut fixture = distant_fixture();
    let session = &mut fixture.session;
    let frame = capture(session);
    session.advance_to(0.5).unwrap();
    let current_view = view(session);
    let token = frame.input_token_at(session, current_view, CENTER).unwrap();
    let occurrence = record(&token, 50, edge(&frame, CENTER, true));
    let mut foreign = session.clone();
    assert_position_rejected(&frame, &mut foreign, current_view, CENTER);
    assert!(foreign
        .submit_native_pointer_input(&mut fixture.store, &token, occurrence)
        .is_err());
    session
        .configure_native_pointer_input(
            NativePointerId {
                source: POINTER.source + 1,
                pointer: POINTER.pointer,
            },
            VIEW,
        )
        .unwrap();
    assert!(session
        .submit_native_pointer_input(&mut fixture.store, &token, occurrence)
        .is_err());
    assert_eq!(session.last_native_event_sequence, None);
}

#[test]
fn compatibility_and_picking_stay_local_with_many_static_rows() {
    let mut objects = vec![button()];
    for index in 0..4096 {
        let mut state = circle(100.0 + f64::from(index % 64) * 4.0);
        state.transform.translation.y = 100.0 + f64::from(index / 64) * 4.0;
        objects.push(state);
    }
    let animated = objects.len();
    objects.push(circle(8.0));
    let mut fixture = tracked_fixture(
        objects,
        animated,
        SemanticObjectTrackProperty::Position,
        position_track(8.0, 12.0),
        RateFunction::Linear,
    );
    let session = &mut fixture.session;
    for tick in 1..=4 {
        let frame = capture(session);
        session.advance_to(f64::from(tick) * 0.25).unwrap();
        let current_view = view(session);
        frame
            .validate_position(session, current_view, CENTER)
            .unwrap();
        let update = session.last_spatial_update_stats();
        assert_eq!(update.full_rebuilds, 0);
        assert_eq!(update.leaves_upserted, 1);
        // A moved leaf removes its old index entry before inserting its replacement.
        assert_eq!(update.leaves_removed, 1);
        let token = frame.input_token_at(session, current_view, CENTER).unwrap();
        let query = session
            .pick_native_pointer_fill(
                &token,
                record(&token, 60, edge(&frame, CENTER, true)),
                |_| true,
            )
            .unwrap();
        assert_eq!(query.outcome(), PointerFillOutcome::Hit(fixture.nodes[0]));
        assert_eq!(query.precise_tests(), 1);
        assert_eq!(query.spatial_stats().candidates_tested, 1);
        assert_eq!(query.spatial_stats().full_scan_fallbacks, 0);
        assert_eq!(session.take_frame_changes().object_indices(), &[animated]);
        assert_eq!(session.last_native_event_sequence, None);
    }
}
