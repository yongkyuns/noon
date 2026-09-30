use super::*;
use crate::integration::{NativeInputModifiers, NativePointerInputKind, NativePointerPosition};
use crate::{ContinuationStep, IndicateOptions, LiveContinuation, LiveProgramStatus, Scene};
use noon_core::{
    AnimationOptions, NativePointerId, NativePointerInput, SemanticClickIndicate,
    SemanticMutationTransaction, SemanticNodeId, SemanticObjectState, SemanticStore,
    StoredGeometry, Vec2, YELLOW,
};

const POINTER: NativePointerId = NativePointerId {
    source: 71,
    pointer: 2,
};

fn input(
    session: &ExecutionSession,
    sequence: u64,
    kind: NativePointerInputKind,
) -> NativePointerInput {
    let token = session.native_pointer_input_token().unwrap();
    NativePointerInput::new(
        sequence,
        token.pointer(),
        token.context(),
        NativeInputModifiers::default(),
        kind,
    )
}

fn click(session: &mut ExecutionSession, sequence: u64, x: f32) {
    let position =
        |x| NativePointerPosition::new(Vec2::new(x, 0.0), Vec2::new(300.0, 150.0)).unwrap();
    let press = input(
        session,
        sequence,
        NativePointerInputKind::Press {
            position: position(x),
            button: 0,
        },
    );
    let token = session.native_pointer_input_token().unwrap();
    session.submit_native_pointer_input(&token, press).unwrap();
    let release = input(
        session,
        sequence + 1,
        NativePointerInputKind::Release {
            position: position(x),
            button: 0,
        },
    );
    let token = session.native_pointer_input_token().unwrap();
    session
        .submit_native_pointer_input(&token, release)
        .unwrap();
}

fn binding(duration: f64) -> SemanticClickIndicate {
    SemanticClickIndicate::new(1.2, YELLOW, duration)
}

fn fixture(
    static_count: usize,
) -> (
    SemanticStore,
    SemanticNodeId,
    SemanticNodeId,
    SemanticNodeId,
    ExecutionSession,
) {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let target = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let mut second_state = SemanticObjectState::new(StoredGeometry::Rectangle {
        size: Vec2::new(1.0, 1.0),
    });
    second_state.transform.translation.x = 5.0;
    let second = store.insert_semantic_object(second_state);
    store.add_semantic_family_member(root, target).unwrap();
    store.add_semantic_family_member(root, second).unwrap();
    for index in 0..static_count {
        let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.1 });
        state.transform.translation.x = 100.0 + index as f64;
        let node = store.insert_semantic_object(state);
        store.add_semantic_family_member(root, node).unwrap();
    }
    let mut declarations = SemanticMutationTransaction::new();
    declarations
        .set_click_indicate(target, Some(binding(0.4)))
        .set_click_indicate(second, Some(binding(0.4)));
    declarations.apply(&mut store).unwrap();
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.configure_native_pointer_input(POINTER, 1).unwrap();
    session.take_frame_changes();
    (store, root, target, second, session)
}

#[test]
fn click_indicate_is_local_and_does_not_change_authored_revisions() {
    let (store, _, target, _, mut session) = fixture(1_000);
    let authored_revision = store.scene_revision();
    let publication = session.publication_context();
    click(&mut session, 1, 0.0);
    assert!(session.interactions_active());
    session.advance_interactions(0.0).unwrap();
    session.advance_interactions(0.2).unwrap();
    assert_eq!(store.scene_revision(), authored_revision);
    assert_eq!(session.frame().time, 0.0);
    assert_eq!(
        session.publication_context().scene_revision(),
        publication.scene_revision()
    );
    assert_eq!(
        session.last_structural_publication_stats().entered_objects,
        0
    );
    assert_eq!(
        session.last_structural_publication_stats().exited_objects,
        0
    );
    let target_object = session.execution_object_id(target).unwrap();
    let target_index = session
        .frame()
        .objects
        .iter()
        .position(|row| row.id == target_object)
        .unwrap();
    assert_eq!(
        session.take_frame_changes().object_indices(),
        &[target_index]
    );
}

#[test]
fn binding_refreshes_through_replace_remove_detach_and_reattach() {
    let (mut store, root, target, _, mut session) = fixture(0);
    let mut replace = SemanticMutationTransaction::new();
    replace.set_click_indicate(target, Some(binding(0.2)));
    session
        .apply_semantic_transaction_at_root(&mut store, root, replace)
        .unwrap();
    click(&mut session, 1, 0.0);
    assert!(session.interactions_active());
    session.advance_interactions(0.0).unwrap();
    session.advance_interactions(0.25).unwrap();
    assert!(!session.interactions_active());
    let mut clear = SemanticMutationTransaction::new();
    clear.set_click_indicate(target, None);
    session
        .apply_semantic_transaction_at_root(&mut store, root, clear)
        .unwrap();
    click(&mut session, 3, 0.0);
    assert!(!session.interactions_active());
    let mut restore = SemanticMutationTransaction::new();
    restore.set_click_indicate(target, Some(binding(0.2)));
    session
        .apply_semantic_transaction_at_root(&mut store, root, restore)
        .unwrap();
    let mut detach = SemanticMutationTransaction::new();
    detach.remove_member(root, target);
    session
        .apply_semantic_transaction_at_root(&mut store, root, detach)
        .unwrap();
    click(&mut session, 5, 0.0);
    assert!(!session.interactions_active());
    let mut attach = SemanticMutationTransaction::new();
    attach.add_member(root, target);
    session
        .apply_semantic_transaction_at_root(&mut store, root, attach)
        .unwrap();
    click(&mut session, 7, 0.0);
    assert!(session.interactions_active());
}

#[test]
fn authored_change_while_active_never_restores_captured_style() {
    let (mut store, root, target, _, mut session) = fixture(0);
    click(&mut session, 1, 0.0);
    session.advance_interactions(0.0).unwrap();
    session.advance_interactions(0.1).unwrap();
    let mut update = SemanticMutationTransaction::new();
    update.replace_style(
        target,
        noon_core::SemanticStyle::from_compact(noon_core::Style {
            fill: Some(noon_core::RED),
            ..noon_core::Style::default()
        }),
    );
    session
        .apply_semantic_transaction_at_root(&mut store, root, update)
        .unwrap();
    session.advance_interactions(0.5).unwrap();
    let object = session.execution_object_id(target).unwrap();
    let index = session
        .frame()
        .objects
        .iter()
        .position(|row| row.id == object)
        .unwrap();
    assert_eq!(
        session.frame().objects[index].style.fill,
        Some(noon_core::RED)
    );
    assert!(!session.interactions_active());
}

struct Finish;
impl LiveContinuation for Finish {
    type Error = std::convert::Infallible;
    fn resume(&mut self, _: &mut crate::LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        Ok(ContinuationStep::Finished)
    }
}

#[test]
fn finished_live_program_accepts_and_advances_source_declared_clicks() {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.8).unwrap();
    circle.set_fill(0.2, 0.4, 0.8, 1.0).unwrap();
    scene.add(&circle).unwrap();
    scene
        .on_click_indicate(&circle, IndicateOptions::default(), AnimationOptions::new())
        .unwrap();
    let mut program = scene.into_live_program(Finish).unwrap();
    program.resume().unwrap();
    let token = program.configure_native_pointer_input(POINTER, 3).unwrap();
    let position = NativePointerPosition::new(Vec2::ZERO, Vec2::new(300.0, 150.0)).unwrap();
    for (sequence, kind) in [
        (
            1,
            NativePointerInputKind::Press {
                position,
                button: 0,
            },
        ),
        (
            2,
            NativePointerInputKind::Release {
                position,
                button: 0,
            },
        ),
    ] {
        let record = NativePointerInput::new(
            sequence,
            token.pointer(),
            token.context(),
            NativeInputModifiers::default(),
            kind,
        );
        program.submit_native_pointer_input(&token, record).unwrap();
    }
    assert_eq!(program.status(), LiveProgramStatus::Finished);
    assert!(program.session().interactions_active());
    program.advance_interactions(0.0).unwrap();
    program.advance_interactions(1.1).unwrap();
    assert_eq!(program.status(), LiveProgramStatus::Finished);
    assert!(!program.session().interactions_active());
}

#[test]
fn click_indicate_preparation_rejects_foreign_runtime_and_retired_targets() {
    let (mut store, root, target, _, mut session) = fixture(0);
    let object = session.execution_object_id(target).unwrap();
    let prepared = session
        .runtime
        .prepare_click_indicate(object, binding(0.4))
        .unwrap()
        .unwrap();
    let mut foreign = session.clone();
    assert!(foreign
        .runtime
        .start_transient_animation(prepared.clone())
        .is_err());
    assert!(!foreign.interactions_active());
    let mut remove = SemanticMutationTransaction::new();
    remove.remove_member(root, target);
    session
        .apply_semantic_transaction_at_root(&mut store, root, remove)
        .unwrap();
    assert!(session.runtime.start_transient_animation(prepared).is_err());
    assert!(!session.interactions_active());
}

#[test]
fn click_action_runs_while_an_unrelated_source_segment_is_pending() {
    let (mut store, root, target, moving, _) = fixture(0);
    let mut endpoint = store.semantic_object_state_checked(moving).unwrap().clone();
    endpoint.transform.translation.x = 7.0;
    let endpoint = store.insert_semantic_object(endpoint);
    let animation = store
        .insert_semantic_transform_animation(moving, endpoint, AnimationOptions::new())
        .unwrap();
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.configure_native_pointer_input(POINTER, 1).unwrap();
    let segment = session
        .activate_animation_segment(
            &store,
            animation,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(noon_core::RateFunction::Linear),
        )
        .unwrap();
    click(&mut session, 1, 0.0);
    assert!(session.interactions_active());
    session.advance_interactions(0.0).unwrap();
    session.advance_to(0.2).unwrap();
    session.advance_interactions(0.2).unwrap();
    assert!(!session.segment_state(segment).is_complete());
    let effect = session.execution_object_id(target).unwrap();
    let moving = session.execution_object_id(moving).unwrap();
    assert_eq!(
        session
            .runtime
            .effective_object(effect)
            .unwrap()
            .transform
            .scale,
        Vec2::new(1.2, 1.2)
    );
    assert!(
        (session
            .runtime
            .effective_object(moving)
            .unwrap()
            .transform
            .translation
            .x
            - 5.4)
            .abs()
            < 1e-5
    );
    assert_eq!(session.frame().time, 0.2);
}

#[test]
fn overlapping_source_scale_supersedes_click_effect_while_unrelated_track_continues() {
    let (mut store, root, target, moving, _) = fixture(0);
    let mut target_endpoint = store.semantic_object_state_checked(target).unwrap().clone();
    target_endpoint.transform.scale = noon_core::SemanticVec3::new(2.0, 2.0, 1.0);
    let target_endpoint = store.insert_semantic_object(target_endpoint);
    let mut moving_endpoint = store.semantic_object_state_checked(moving).unwrap().clone();
    moving_endpoint.transform.translation.x = 7.0;
    let moving_endpoint = store.insert_semantic_object(moving_endpoint);
    let target_animation = store
        .insert_semantic_transform_animation(target, target_endpoint, AnimationOptions::new())
        .unwrap();
    let moving_animation = store
        .insert_semantic_transform_animation(moving, moving_endpoint, AnimationOptions::new())
        .unwrap();
    let parallel = store
        .insert_semantic_parallel_animation(
            &[target_animation, moving_animation],
            AnimationOptions::new(),
        )
        .unwrap();
    let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
    session.configure_native_pointer_input(POINTER, 1).unwrap();

    click(&mut session, 1, 0.0);
    session.advance_interactions(0.0).unwrap();
    session.advance_interactions(0.1).unwrap();
    assert!(session.interactions_active());
    let segment = session
        .activate_animation_segment(
            &store,
            parallel,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(noon_core::RateFunction::Linear),
        )
        .unwrap();
    session.advance_segment_to(segment, 0.5).unwrap();
    session.advance_interactions(0.2).unwrap();

    let target_row = session
        .runtime
        .effective_object(session.execution_object_id(target).unwrap())
        .unwrap();
    assert!(
        !session.interactions_active(),
        "overlapping authored driver must retire the click effect"
    );
    // The source segment starts from the currently visible 1.1 scale, then
    // takes ownership; the old click driver must not add its own scale on top.
    assert_eq!(target_row.transform.scale, Vec2::new(1.55, 1.55));
    assert_ne!(
        target_row.style.fill,
        Some(YELLOW),
        "the source owns the color transition"
    );
    let moving_row = session
        .runtime
        .effective_object(session.execution_object_id(moving).unwrap())
        .unwrap();
    assert!((moving_row.transform.translation.x - 6.0).abs() < 1e-5);
    assert_eq!(session.frame().time, 0.5);

    session
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    session.complete_segment(&mut store, segment).unwrap();
    assert_eq!(
        session
            .runtime
            .effective_object(session.execution_object_id(target).unwrap())
            .unwrap()
            .transform
            .scale,
        Vec2::new(2.0, 2.0)
    );
    assert_eq!(
        session
            .runtime
            .effective_object(session.execution_object_id(target).unwrap())
            .unwrap()
            .style
            .fill,
        Some(noon_core::WHITE)
    );
    assert!(!session.interactions_active());
    click(&mut session, 3, 0.0);
    assert!(
        session.interactions_active(),
        "fresh click may acquire the released channels"
    );
}

#[test]
fn completed_create_and_wait_allow_click_with_nonreplayable_drag_policy() {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.9).unwrap();
    circle.set_fill(0.0, 0.0, 1.0, 0.78).unwrap();
    let mut rectangle = scene.rectangle(2.2, 1.6).unwrap();
    rectangle.set_translation(4.0, 0.0).unwrap();
    let mut session = scene.execution_session().unwrap();
    session.begin_replay_retention(Default::default()).unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_create_parallel(
            &[
                (&circle, AnimationOptions::new()),
                (&rectangle, AnimationOptions::new()),
            ],
            AnimationOptions::new().run_time(1.4),
        )
        .unwrap();
    session
        .advance_segment_to(segment, segment.end_time())
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    let wait = scene.live(&mut session).wait_segment(0.5).unwrap();
    session.advance_segment_to(wait, wait.end_time()).unwrap();
    scene.live(&mut session).complete_segment(wait).unwrap();
    scene
        .live(&mut session)
        .on_click_indicate(
            &circle,
            IndicateOptions::default(),
            AnimationOptions::new().run_time(0.4),
        )
        .unwrap();
    scene
        .live(&mut session)
        .set_translation_drag_targets([&rectangle])
        .unwrap();
    assert!(session.seal_replay().is_err());
    session.configure_native_pointer_input(POINTER, 1).unwrap();
    click(&mut session, 1, 0.0);
    assert!(
        session.interactions_active(),
        "first-pass completed Create must not suppress native click"
    );
}
