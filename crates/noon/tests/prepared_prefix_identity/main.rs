#![cfg(feature = "prepared-prefix")]
//! Generation-qualified actions after delivery of genuinely new execution slots.
//! Direct effects and pointer dispatch share the same historical session.

use noon::{ExecutionSession, PrefixError, PrefixPublisher, PrefixReader};
use noon_core::{
    Color, ObjectId, SemanticClickIndicate, SemanticMutationTransaction, SemanticNodeCreation,
    SemanticNodeId, SemanticObjectState, SemanticPaint, SemanticStore, StoredGeometry, Vec2,
};
use noon_runtime::{PrefixBatch, ReplayLimits};

mod admission;
mod pointer;
mod terminal;

const A: SemanticClickIndicate = SemanticClickIndicate::new(1.25, Color::YELLOW, 0.5);
const B: SemanticClickIndicate = SemanticClickIndicate::new(1.75, Color::RED, 1.0);

fn state(binding: SemanticClickIndicate) -> SemanticObjectState {
    let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.65 });
    state.style.fill = Some(SemanticPaint::Solid(Color::BLUE));
    state.set_click_action(Some(noon_core::SemanticClickAction::Indicate(binding)));
    state
}

struct Producer {
    store: SemanticStore,
    root: SemanticNodeId,
    original: SemanticNodeId,
    object: ObjectId,
    session: ExecutionSession,
}

impl Producer {
    fn new() -> Self {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let original = store.insert_semantic_object(state(A));
        store.add_member(root, original).unwrap();
        store.attach_to_scene(root).unwrap();
        let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
        let object = session.execution_object_id(original).unwrap();
        session
            .begin_replay_retention(ReplayLimits {
                revisions: 128,
                payloads: 2048,
            })
            .unwrap();
        Self {
            store,
            root,
            original,
            object,
            session,
        }
    }

    fn bootstrap(&mut self) -> (PrefixPublisher, PrefixReader) {
        let boundary = self.session.wait_segment(1.0).unwrap();
        self.session.advance_to(boundary.end_time()).unwrap();
        self.session.create_prepared_prefix(boundary).unwrap()
    }

    fn publish(&mut self, publisher: &mut PrefixPublisher) -> PrefixBatch {
        let boundary = self.session.wait_segment(1.0).unwrap();
        self.session.advance_to(boundary.end_time()).unwrap();
        self.session
            .publish_prepared_prefix(boundary, publisher)
            .unwrap()
    }

    fn delete(&mut self) {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.remove_node(self.original);
        self.session
            .apply_semantic_transaction(&mut self.store, transaction)
            .unwrap();
        assert!(self.store.node(self.original).is_none());
        assert_eq!(self.session.execution_object_id(self.original), None);
    }

    fn replacement(&mut self) -> ObjectId {
        let mut transaction = SemanticMutationTransaction::new();
        let token = transaction.create_node(SemanticNodeCreation::object(state(B)));
        transaction.add_member(self.root, token);
        let result = self
            .session
            .apply_semantic_transaction(&mut self.store, transaction)
            .unwrap();
        let replacement = result.resolve(token).unwrap();
        assert_eq!(replacement.slot(), self.original.slot());
        assert_ne!(replacement.generation(), self.original.generation());
        let object = self.session.execution_object_id(replacement).unwrap();
        assert_ne!(object, self.object);
        object
    }
}

fn append(reader: &mut PrefixReader, batch: PrefixBatch) {
    reader
        .reserve_object_capacity(batch.required_object_capacity())
        .unwrap();
    reader.append(batch).unwrap();
}

fn absent(reader: &mut PrefixReader, object: ObjectId) {
    let frame = reader.frame().clone();
    let history = reader.replay_stats();
    let context = reader.take_renderer_publication().context();
    assert!(matches!(
        reader.prepare_bound_click_indicate(object),
        Err(PrefixError::Interaction(_))
    ));
    assert_eq!(reader.frame(), &frame);
    assert_eq!(reader.replay_stats(), history);
    assert_eq!(reader.take_renderer_publication().context(), context);
}

fn exercise(
    reader: &mut PrefixReader,
    object: ObjectId,
    binding: SemanticClickIndicate,
    wall_time: f64,
) {
    let baseline = reader.frame().clone();
    let history = reader.replay_stats();
    let context = reader.take_renderer_publication().context();
    let prepared = reader
        .prepare_bound_click_indicate(object)
        .unwrap()
        .unwrap();
    reader.start_transient_animation(prepared).unwrap();
    reader.advance_interactions(wall_time).unwrap();
    reader
        .advance_interactions(wall_time + binding.run_time() * 0.5)
        .unwrap();
    let row = reader
        .frame()
        .objects
        .iter()
        .find(|row| row.id == object)
        .unwrap();
    assert_eq!(row.style.fill, Some(binding.color()));
    assert_eq!(
        row.transform.scale,
        Vec2::new(binding.scale_factor() as f32, binding.scale_factor() as f32)
    );
    assert_eq!(reader.frame().time, baseline.time);
    assert!(reader.interactions_active());
    reader
        .advance_interactions(wall_time + binding.run_time())
        .unwrap();
    assert!(!reader.interactions_active());
    assert_eq!(reader.frame(), &baseline);
    assert_eq!(reader.replay_stats(), history);
    let restored = reader.take_renderer_publication().context();
    assert_eq!(restored.scene_revision(), context.scene_revision());
    assert_eq!(restored.execution_revision(), context.execution_revision());
}

fn delivered_replacement() -> (PrefixReader, ObjectId, ObjectId) {
    let mut source = Producer::new();
    let old = source.object;
    let (mut publisher, mut reader) = source.bootstrap();
    source.delete();
    append(&mut reader, source.publish(&mut publisher));
    let new = source.replacement();
    let batch = source.publish(&mut publisher);
    assert_eq!(batch.stats().new_object_slots, 1);
    append(&mut reader, batch);
    // Neither the mutable SemanticStore nor its execution index survives here.
    drop(source);
    drop(publisher);
    (reader, old, new)
}

#[test]
fn prefix_identity_reused_slot_retains_historical_actions_after_source_drop() {
    let (mut reader, old, new) = delivered_replacement();
    for (iteration, (time, target)) in [
        (0.5, Some((old, new, A))),
        (1.5, None),
        (2.5, Some((new, old, B))),
        (0.5, Some((old, new, A))),
        (2.5, Some((new, old, B))),
    ]
    .into_iter()
    .enumerate()
    {
        reader.seek(time).unwrap();
        if let Some((present, missing, binding)) = target {
            absent(&mut reader, missing);
            exercise(
                &mut reader,
                present,
                binding,
                100.0 + iteration as f64 * 10.0,
            );
        } else {
            absent(&mut reader, old);
            absent(&mut reader, new);
            assert!(!reader.interactions_active());
        }
    }
}

#[test]
fn prefix_identity_new_slot_delivery_preserves_active_effect_and_displayed_publication() {
    let mut source = Producer::new();
    let old = source.object;
    let (mut publisher, mut reader) = source.bootstrap();
    reader.seek(0.5).unwrap();
    let baseline = reader.frame().clone();
    let prepared = reader.prepare_bound_click_indicate(old).unwrap().unwrap();
    reader.start_transient_animation(prepared).unwrap();
    reader.advance_interactions(10.0).unwrap();
    reader.advance_interactions(10.125).unwrap();
    let active = reader.frame().clone();
    let context = reader.take_renderer_publication().context();

    source.delete();
    append(&mut reader, source.publish(&mut publisher));
    let new = source.replacement();
    let batch = source.publish(&mut publisher);
    assert_eq!(batch.stats().new_object_slots, 1);
    reader
        .reserve_object_capacity(batch.required_object_capacity())
        .unwrap();
    assert_eq!(reader.frame(), &active);
    assert_eq!(reader.take_renderer_publication().context(), context);
    reader.append(batch).unwrap();
    assert_eq!(reader.frame(), &active);
    assert_eq!(reader.take_renderer_publication().context(), context);
    assert_eq!(reader.frontier(), 3.0);
    assert!(reader.interactions_active());
    absent(&mut reader, new);
    drop(source);
    drop(publisher);

    reader.advance_interactions(10.5).unwrap();
    assert!(!reader.interactions_active());
    assert_eq!(reader.frame(), &baseline);
    reader.advance_to(2.5).unwrap();
    absent(&mut reader, old);
    exercise(&mut reader, new, B, 20.0);
}

#[test]
fn prefix_identity_unavailable_time_does_not_materialize_future_slots_or_cancel_effect() {
    let mut source = Producer::new();
    let old = source.object;
    let (mut publisher, mut reader) = source.bootstrap();
    reader.seek(0.5).unwrap();
    let prepared = reader.prepare_bound_click_indicate(old).unwrap().unwrap();
    reader.start_transient_animation(prepared).unwrap();
    reader.advance_interactions(10.0).unwrap();
    reader.advance_interactions(10.125).unwrap();
    let before_delivery = reader.frame().clone();
    source.delete();
    append(&mut reader, source.publish(&mut publisher));
    let new = source.replacement();
    append(&mut reader, source.publish(&mut publisher));
    assert_eq!(reader.frame(), &before_delivery);
    let history = reader.replay_stats();
    let context = reader.take_renderer_publication().context();

    for time in [-1.0, f64::NAN, f64::INFINITY, reader.frontier()] {
        assert!(matches!(
            reader.seek(time),
            Err(PrefixError::OutsideRange { .. })
        ));
        assert_eq!(reader.frame(), &before_delivery);
        assert_eq!(reader.replay_stats(), history);
        assert_eq!(reader.take_renderer_publication().context(), context);
        assert!(reader.interactions_active());
        assert!(matches!(
            reader.advance_to(time),
            Err(PrefixError::OutsideRange { .. })
        ));
        assert_eq!(reader.frame(), &before_delivery);
        assert_eq!(reader.replay_stats(), history);
        assert_eq!(reader.take_renderer_publication().context(), context);
        assert!(reader.interactions_active());
    }
    assert!(matches!(
        reader.advance_to(0.25),
        Err(PrefixError::Evaluation(_))
    ));
    assert_eq!(reader.frame(), &before_delivery);
    assert_eq!(reader.replay_stats(), history);
    assert_eq!(reader.take_renderer_publication().context(), context);
    assert!(reader.interactions_active());
    absent(&mut reader, new);
    reader.advance_interactions(10.5).unwrap();
    assert!(!reader.interactions_active());
}

#[test]
fn prefix_identity_old_preparation_cannot_act_on_the_replacement_generation() {
    let (mut reader, old, new) = delivered_replacement();
    reader.seek(0.5).unwrap();
    let prepared = reader.prepare_bound_click_indicate(old).unwrap().unwrap();
    reader.seek(2.5).unwrap();
    let frame = reader.frame().clone();
    let history = reader.replay_stats();
    let context = reader.take_renderer_publication().context();
    assert!(matches!(
        reader.start_transient_animation(prepared),
        Err(PrefixError::Interaction(_))
    ));
    assert_eq!(reader.frame(), &frame);
    assert_eq!(reader.replay_stats(), history);
    assert_eq!(reader.take_renderer_publication().context(), context);
    assert!(!reader.interactions_active());
    exercise(&mut reader, new, B, 20.0);
}

#[test]
fn prefix_identity_foreign_stream_rejection_preserves_the_active_reader() {
    let mut source = Producer::new();
    let old = source.object;
    let (mut publisher, mut reader) = source.bootstrap();
    reader.seek(0.5).unwrap();
    let prepared = reader.prepare_bound_click_indicate(old).unwrap().unwrap();
    reader.start_transient_animation(prepared).unwrap();
    reader.advance_interactions(10.0).unwrap();
    reader.advance_interactions(10.125).unwrap();
    let mut foreign = Producer::new();
    assert_eq!(
        foreign.object, old,
        "numeric IDs alone are not stream provenance"
    );
    let (mut foreign_publisher, _) = foreign.bootstrap();
    foreign.delete();
    foreign.replacement();
    let batch = foreign.publish(&mut foreign_publisher);
    reader
        .reserve_object_capacity(batch.required_object_capacity())
        .unwrap();
    let frame = reader.frame().clone();
    let history = reader.replay_stats();
    let context = reader.take_renderer_publication().context();
    let frontier = reader.frontier();
    assert!(matches!(
        reader.append(batch),
        Err(PrefixError::UnexpectedBatch)
    ));
    assert_eq!(reader.frame(), &frame);
    assert_eq!(reader.replay_stats(), history);
    assert_eq!(reader.take_renderer_publication().context(), context);
    assert_eq!(reader.frontier(), frontier);
    assert!(reader.interactions_active());

    // A rejected foreign batch has not consumed this stream's next revision.
    source.delete();
    append(&mut reader, source.publish(&mut publisher));
    assert_eq!(reader.frontier(), 2.0);
    assert_eq!(reader.frame(), &frame);
    assert_eq!(reader.take_renderer_publication().context(), context);
    reader.advance_interactions(10.5).unwrap();
    assert!(!reader.interactions_active());
    reader.seek(1.5).unwrap();
    absent(&mut reader, old);
}
