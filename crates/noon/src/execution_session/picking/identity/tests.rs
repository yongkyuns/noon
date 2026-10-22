use super::*;
use crate::execution_session::{
    ExecutionSessionInputError, NativePointerInputPublication, NativePointerInputToken,
    PointerFillOutcome,
};
use noon_core::{
    Color, NativeInputModifiers, NativePointerId, NativePointerInput, NativePointerInputKind,
    NativePointerPosition, SemanticClickIndicate, SemanticMutationTransaction, SemanticNodeCreation,
    SemanticObjectState, SemanticPaint, SemanticStore, StoredGeometry, Vec2,
};
use noon_runtime::ReplayLimits;

mod hover;

const A: SemanticClickIndicate = SemanticClickIndicate::new(1.25, Color::YELLOW, 0.5);
const B: SemanticClickIndicate = SemanticClickIndicate::new(1.75, Color::RED, 1.0);

struct Fixture {
    store: SemanticStore,
    root: SemanticNodeId,
    node: SemanticNodeId,
    object: ObjectId,
    session: ExecutionSession,
}

fn state(binding: SemanticClickIndicate) -> SemanticObjectState {
    let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.65 });
    state.style.fill = Some(SemanticPaint::Solid(Color::BLUE));
    state.set_click_indicate(Some(binding));
    state
}

impl Fixture {
    fn new() -> Self {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let node = store.insert_semantic_object(state(A));
        store.add_member(root, node).unwrap();
        store.attach_to_scene(root).unwrap();
        let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
        let object = session.execution_object_id(node).unwrap();
        session
            .begin_replay_retention(ReplayLimits {
                revisions: 128,
                payloads: 2048,
            })
            .unwrap();
        Self {
            store,
            root,
            node,
            object,
            session,
        }
    }

    fn wait(&mut self) {
        let boundary = self.session.wait_segment(1.0).unwrap();
        self.session.advance_to(boundary.end_time()).unwrap();
    }

    fn delete(&mut self) {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.remove_node(self.node);
        self.session
            .apply_semantic_transaction(&mut self.store, transaction)
            .unwrap();
        assert!(self.store.node(self.node).is_none());
        assert_eq!(self.session.execution_object_id(self.node), None);
        assert_eq!(self.session.interaction_execution_object_id(self.node), None);
        assert_eq!(self.session.interaction_semantic_object_id(self.object), None);
    }

    fn replacement(&mut self) -> (SemanticNodeId, ObjectId) {
        let mut transaction = SemanticMutationTransaction::new();
        let token = transaction.create_node(SemanticNodeCreation::object(state(B)));
        transaction.add_member(self.root, token);
        let result = self
            .session
            .apply_semantic_transaction(&mut self.store, transaction)
            .unwrap();
        let node = result.resolve(token).unwrap();
        assert_eq!(node.slot(), self.node.slot());
        assert_ne!(node.generation(), self.node.generation());
        (node, self.session.execution_object_id(node).unwrap())
    }

    fn seal(mut self) -> ExecutionSession {
        self.session.seal_replay().unwrap();
        configure(&mut self.session);
        // The authored store and its deleted node are not needed by playback.
        self.session
    }
}

fn configure(session: &mut ExecutionSession) {
    session.enable_pointer_fill_selection(4.0).unwrap();
    session
        .configure_native_pointer_input(
            NativePointerId {
                source: 71,
                pointer: 2,
            },
            1,
        )
        .unwrap();
}

fn input(token: &NativePointerInputToken, sequence: u64, press: bool) -> NativePointerInput {
    let position = NativePointerPosition::new(Vec2::ZERO, Vec2::new(100.0, 100.0)).unwrap();
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

fn submit(
    session: &mut ExecutionSession,
    sequence: u64,
    press: bool,
) -> NativePointerInputPublication {
    let token = session.native_pointer_input_token().unwrap();
    session
        .submit_native_pointer_input(&token, input(&token, sequence, press))
        .unwrap()
}

fn click(session: &mut ExecutionSession, sequence: u64) -> NativePointerInputPublication {
    submit(session, sequence, true);
    submit(session, sequence + 1, false)
}

fn exercise_action(
    session: &mut ExecutionSession,
    object: ObjectId,
    binding: SemanticClickIndicate,
    wall_time: f64,
) {
    let baseline = session.frame().clone();
    let history = session.replay_stats();
    let context = session.publication_context();
    assert!(session.interactions_active());
    session.advance_interactions(wall_time).unwrap();
    session
        .advance_interactions(wall_time + binding.run_time() * 0.5)
        .unwrap();
    let index = session.runtime.frame_index_for_object(object).unwrap();
    let row = &session.frame().objects[index];
    assert_eq!(row.style.fill, Some(binding.color()));
    assert_eq!(
        row.transform.scale,
        Vec2::new(binding.scale_factor() as f32, binding.scale_factor() as f32)
    );
    assert_eq!(session.frame().time, baseline.time);
    session
        .advance_interactions(wall_time + binding.run_time())
        .unwrap();
    assert!(!session.interactions_active());
    assert_eq!(session.frame(), &baseline);
    assert_eq!(session.replay_stats(), history);
    assert_eq!(
        session.publication_context().scene_revision(),
        context.scene_revision()
    );
    assert_eq!(
        session.publication_context().execution_revision(),
        context.execution_revision()
    );
}

#[test]
fn historical_pointer_identity_deleted_source_target_can_be_picked_selected_and_animated() {
    let mut fixture = Fixture::new();
    fixture.wait();
    fixture.delete();
    fixture.wait();
    let (node, object) = (fixture.node, fixture.object);
    let mut session = fixture.seal();
    session.seek(0.5).unwrap();
    assert_eq!(session.execution_object_id(node), None);
    assert_eq!(session.interaction_execution_object_id(node), Some(object));
    assert_eq!(session.interaction_semantic_object_id(object), Some(node));

    let token = session.native_pointer_input_token().unwrap();
    let query = session
        .pick_native_pointer_fill(&token, input(&token, 0, true), |_| true)
        .unwrap();
    assert_eq!(query.outcome(), PointerFillOutcome::Hit(node));
    assert_eq!(query.precise_tests(), 1);
    assert_eq!(query.spatial_stats().full_scan_fallbacks, 0);
    // Picking did not acknowledge the occurrence: the same sequence is admitted.
    let receipt = click(&mut session, 0);
    assert_eq!(receipt.selection_click().unwrap().target(), Some(node));
    assert_eq!(session.selected_pointer_target(), Some(node));
    assert_eq!(session.pointer_selection_highlight().unwrap().target, node);
    exercise_action(&mut session, object, A, 10.0);
    assert_eq!(session.selected_pointer_target(), Some(node));
    assert_eq!(session.execution_object_id(node), None);
}

#[test]
fn historical_pointer_identity_slot_reuse_never_aliases_old_and_future_targets() {
    let mut fixture = Fixture::new();
    fixture.wait();
    fixture.delete();
    fixture.wait();
    let (replacement, replacement_object) = fixture.replacement();
    fixture.wait();
    let (node, object) = (fixture.node, fixture.object);
    let mut session = fixture.seal();

    for (iteration, (time, expected)) in [
        (0.5, Some((node, object, A))),
        (1.5, None),
        (2.5, Some((replacement, replacement_object, B))),
        (0.5, Some((node, object, A))),
        (2.5, Some((replacement, replacement_object, B))),
    ]
    .into_iter()
    .enumerate()
    {
        session.seek(time).unwrap();
        let target = expected.map(|(node, _, _)| node);
        assert_eq!(
            session.interaction_execution_object_id(node),
            (target == Some(node)).then_some(object)
        );
        assert_eq!(
            session.interaction_execution_object_id(replacement),
            (target == Some(replacement)).then_some(replacement_object)
        );
        let receipt = click(&mut session, iteration as u64 * 2);
        assert_eq!(receipt.selection_click().unwrap().target(), target);
        assert_eq!(session.selected_pointer_target(), target);
        if let Some((_, object, binding)) = expected {
            exercise_action(&mut session, object, binding, 100.0 + iteration as f64 * 10.0);
        } else {
            assert!(!session.interactions_active());
            assert!(session.pointer_selection_highlight().is_none());
        }
        assert_eq!(session.execution_object_id(node), None);
    }
}

#[test]
fn historical_pointer_identity_seek_cancels_gestures_and_stale_input_is_atomic() {
    let mut fixture = Fixture::new();
    fixture.wait();
    fixture.delete();
    fixture.wait();
    let node = fixture.node;
    let mut session = fixture.seal();
    session.seek(0.5).unwrap();
    submit(&mut session, 0, true);
    let stale = session.native_pointer_input_token().unwrap();
    session.seek(0.75).unwrap();
    let baseline = session.frame().clone();
    let context = session.publication_context();
    let history = session.replay_stats();
    assert!(matches!(
        session.submit_native_pointer_input(&stale, input(&stale, 1, false)),
        Err(ExecutionSessionInputError::StalePointerPublication { .. })
    ));
    assert_eq!(session.frame(), &baseline);
    assert_eq!(session.publication_context(), context);
    assert_eq!(session.replay_stats(), history);
    // Rejection consumed no sequence; seek cancelled the earlier press.
    assert!(submit(&mut session, 1, false).selection_click().is_none());
    assert!(!session.interactions_active());
    assert_eq!(
        click(&mut session, 2).selection_click().unwrap().target(),
        Some(node)
    );
    session.seek(0.75).unwrap();
    assert!(!session.interactions_active());
    assert_eq!(session.selected_pointer_target(), None);
    submit(&mut session, 4, true);
    session.seek(0.75).unwrap();
    assert!(submit(&mut session, 5, false).selection_click().is_none());
    assert!(!session.interactions_active());
}

#[test]
fn historical_pointer_identity_queries_do_not_publish_or_admit_future_rows() {
    let mut fixture = Fixture::new();
    fixture.wait();
    fixture.delete();
    fixture.wait();
    let (replacement, replacement_object) = fixture.replacement();
    fixture.wait();
    let (node, object, root) = (fixture.node, fixture.object, fixture.root);
    let mut session = fixture.seal();
    session.seek(0.5).unwrap();
    session.take_frame_changes();
    let baseline = session.frame().clone();
    let context = session.publication_context();
    let history = session.replay_stats();
    for _ in 0..32 {
        assert_eq!(session.interaction_execution_object_id(node), Some(object));
        assert_eq!(session.interaction_semantic_object_id(object), Some(node));
        assert_eq!(session.interaction_execution_object_id(replacement), None);
        assert_eq!(
            session.interaction_semantic_object_id(replacement_object),
            None
        );
        assert_eq!(session.interaction_execution_object_id(root), None);
        assert_eq!(
            session.interaction_semantic_object_id(ObjectId::new(u64::MAX)),
            None
        );
    }
    assert_eq!(session.frame(), &baseline);
    assert_eq!(session.publication_context(), context);
    assert_eq!(session.replay_stats(), history);
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn historical_pointer_identity_forward_removal_retires_selection_and_effect() {
    let mut fixture = Fixture::new();
    fixture.wait();
    fixture.delete();
    fixture.wait();
    let (node, object) = (fixture.node, fixture.object);
    let mut session = fixture.seal();
    session.seek(0.5).unwrap();
    click(&mut session, 0);
    session.advance_interactions(10.0).unwrap();
    session.advance_to(0.75).unwrap();
    session.advance_interactions(10.125).unwrap();
    assert_eq!(session.selected_pointer_target(), Some(node));
    assert!(session.interactions_active());
    session.advance_to(1.5).unwrap();
    session.advance_interactions(10.25).unwrap();
    assert_eq!(session.interaction_semantic_object_id(object), None);
    assert_eq!(session.selected_pointer_target(), None);
    assert!(session.pointer_selection_highlight().is_none());
    assert!(!session.interactions_active());
}

#[test]
fn historical_pointer_identity_discard_returns_to_current_source_membership() {
    let mut fixture = Fixture::new();
    fixture.wait();
    fixture.delete();
    fixture.wait();
    let (replacement, replacement_object) = fixture.replacement();
    fixture.wait();
    fixture.session.seal_replay().unwrap();
    configure(&mut fixture.session);
    fixture.session.seek(0.5).unwrap();
    click(&mut fixture.session, 0);
    fixture.session.advance_interactions(10.0).unwrap();
    fixture.session.advance_interactions(10.25).unwrap();
    fixture.session.discard_replay_retention();
    assert!(!fixture.session.replay_scope_active());
    assert!(!fixture.session.interactions_active());
    assert_eq!(fixture.session.selected_pointer_target(), None);
    assert_eq!(fixture.session.frame().time, 3.0);
    assert_eq!(
        fixture.session.interaction_execution_object_id(fixture.node),
        None
    );
    assert_eq!(
        fixture.session.interaction_execution_object_id(replacement),
        Some(replacement_object)
    );
    assert_eq!(fixture.session.execution_object_id(fixture.node), None);
    assert!(fixture.store.node(fixture.node).is_none());
    let index = fixture
        .session
        .runtime
        .frame_index_for_object(replacement_object)
        .unwrap();
    assert_eq!(
        fixture.session.frame().objects[index].style.fill,
        Some(Color::BLUE)
    );
}
