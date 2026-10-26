//! Browser receipt lifecycles exercised against a real independent prefix.
//! `present` explicitly models host acknowledgement; these tests do not assert
//! physical GPU/DOM presentation. No target is chosen by the browser adapter.
use super::*;
use crate::direct_pointer_presentation::DirectPointerPresentation;
use crate::worker_pointer_presentation::{WorkerPointerPresentation, WorkerPointerReceipt};
use crate::PointerPresentationView;
use noon::{PrefixPublisher, PrefixReader};
use noon_core::{
    Color, ObjectId, SemanticClickIndicate, SemanticMutationTransaction, SemanticNodeCreation,
    SemanticNodeId, SemanticObjectState, SemanticPaint, SemanticStore, StoredGeometry,
};
use noon_runtime::ReplayLimits;

const SIZE: Vec2 = Vec2::new(800.0, 400.0);
const A: SemanticClickIndicate = SemanticClickIndicate::new(1.25, Color::YELLOW, 0.5);
const B: SemanticClickIndicate = SemanticClickIndicate::new(1.5, Color::RED, 0.5);

fn state(action: SemanticClickIndicate) -> SemanticObjectState {
    let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.65 });
    state.style.fill = Some(SemanticPaint::Solid(Color::BLUE));
    state.set_click_action(Some(noon_core::SemanticClickAction::Indicate(action)));
    state
}

struct Source {
    store: SemanticStore,
    root: SemanticNodeId,
    node: SemanticNodeId,
    session: ExecutionSession,
}
impl Source {
    fn new() -> (Self, PrefixPublisher, Host) {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let node = store.insert_semantic_object(state(A));
        store.add_member(root, node).unwrap();
        store.attach_to_scene(root).unwrap();
        let mut session = ExecutionSession::from_semantic_root(&store, root).unwrap();
        session
            .begin_replay_retention(ReplayLimits {
                revisions: 128,
                payloads: 2048,
            })
            .unwrap();
        let boundary = session.wait_segment(1.0).unwrap();
        session.advance_to(1.0).unwrap();
        let (publisher, mut reader) = session.create_prepared_prefix(boundary).unwrap();
        reader.seek(0.5).unwrap();
        reader.enable_pointer_fill_selection(4.0).unwrap();
        let mut presentation = DirectPointerPresentation::default();
        presentation.set_view(1, SIZE).unwrap();
        (
            Self {
                store,
                root,
                node,
                session,
            },
            publisher,
            Host {
                reader,
                presentation,
                binding: None,
                sequence: 0,
            },
        )
    }

    fn replace_and_finish(
        &mut self,
        publisher: &mut PrefixPublisher,
        reader: &mut PrefixReader,
    ) -> SemanticNodeId {
        let mut deletion = SemanticMutationTransaction::new();
        deletion.remove_node(self.node);
        self.session
            .apply_semantic_transaction(&mut self.store, deletion)
            .unwrap();
        let mut creation = SemanticMutationTransaction::new();
        let token = creation.create_node(SemanticNodeCreation::object(state(B)));
        creation.add_member(self.root, token);
        let result = self
            .session
            .apply_semantic_transaction(&mut self.store, creation)
            .unwrap();
        let replacement = result.resolve(token).unwrap();
        assert_eq!(replacement.slot(), self.node.slot());
        assert_ne!(replacement.generation(), self.node.generation());
        let boundary = self.session.wait_segment(1.0).unwrap();
        self.session.advance_to(2.0).unwrap();
        self.session.seal_replay().unwrap();
        let terminal = self
            .session
            .finish_prepared_prefix(boundary, publisher)
            .unwrap();
        reader
            .reserve_object_capacity(terminal.required_object_capacity())
            .unwrap();
        reader.append(terminal).unwrap();
        replacement
    }
}

struct Host {
    reader: PrefixReader,
    presentation: DirectPointerPresentation,
    binding: Option<BrowserPointerBinding>,
    sequence: u64,
}
impl Host {
    fn present(&mut self) {
        let frame = self
            .presentation
            .capture(&self.reader, self.reader.inspection_camera().unwrap())
            .unwrap();
        self.reader.take_renderer_publication();
        // Model the existing canvas host's successful submit/present boundary.
        self.presentation.did_present(frame);
        if let Some(frame) = &self.presentation.presented {
            self.reader.observe_pointer_frame(frame).unwrap();
        }
    }
    fn send(&mut self, kind: BrowserPointerKind, source: u64) -> Result<bool, String> {
        self.presentation
            .submit(
                &mut self.reader,
                &mut self.binding,
                &mut self.sequence,
                wire(kind, source),
            )
            .map(|publication| publication.is_some())
    }
    fn click(&mut self, source: u64) {
        assert!(self.send(BrowserPointerKind::Press, source).unwrap());
        assert!(self.send(BrowserPointerKind::Release, source).unwrap());
    }
}
fn wire(kind: BrowserPointerKind, source: u64) -> BrowserPointerInput {
    let cancel = matches!(
        kind,
        BrowserPointerKind::Cancel
            | BrowserPointerKind::FocusLost
            | BrowserPointerKind::CaptureLost
    );
    BrowserPointerInput {
        kind,
        source_id: source,
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
fn effect(reader: &mut PrefixReader, object: ObjectId, action: SemanticClickIndicate, wall: f64) {
    let before = reader.frame().clone();
    let history = reader.replay_stats();
    assert!(reader.interactions_active());
    reader.advance_interactions(wall).unwrap();
    reader.advance_interactions(wall + 0.25).unwrap();
    let row = reader
        .frame()
        .objects
        .iter()
        .find(|row| row.id == object)
        .unwrap();
    assert_eq!(row.style.fill, Some(action.color()));
    assert_eq!(reader.frame().time, before.time);
    reader.advance_interactions(wall + 0.5).unwrap();
    assert!(!reader.interactions_active());
    assert_eq!(reader.frame(), &before);
    assert_eq!(reader.replay_stats(), history);
}

#[test]
fn prefix_browser_capture_is_not_presentation_and_click_works_after_source_drop() {
    let (source, publisher, mut host) = Source::new();
    let node = source.node;
    let object = source.session.execution_object_id(node).unwrap();
    let before = host.reader.frame().clone();
    let context = host.reader.publication_context();
    let captured = host
        .presentation
        .capture(&host.reader, host.reader.inspection_camera().unwrap())
        .unwrap();
    assert!(captured.is_some());
    assert!(!host.send(BrowserPointerKind::Press, 1).unwrap());
    assert_eq!(host.sequence, 0);
    assert!(host.binding.is_none());
    assert_eq!(host.reader.frame(), &before);
    assert_eq!(host.reader.publication_context(), context);
    drop(source);
    drop(publisher);
    host.present();
    host.click(2);
    assert_eq!(host.reader.selected_pointer_target(), Some(node));
    effect(&mut host.reader, object, A, 10.0);
}

#[test]
fn prefix_browser_future_terminal_delivery_preserves_press_and_uses_historical_binding() {
    let (mut source, mut publisher, mut host) = Source::new();
    let old = source.node;
    let old_object = source.session.execution_object_id(old).unwrap();
    host.present();
    assert!(host.send(BrowserPointerKind::Press, 1).unwrap());
    let frame = host.reader.frame().clone();
    let receipt = host.presentation.presented.clone();
    let replacement = source.replace_and_finish(&mut publisher, &mut host.reader);
    let replacement_object = source.session.execution_object_id(replacement).unwrap();
    assert_eq!(host.reader.frame(), &frame);
    assert_eq!(host.reader.end_time(), Some(2.0));
    assert_eq!(host.presentation.presented, receipt);
    assert!(!host.presentation.needs_refresh(&host.reader));
    drop(source);
    drop(publisher);
    assert!(host.send(BrowserPointerKind::Release, 1).unwrap());
    assert_eq!(host.reader.selected_pointer_target(), Some(old));
    effect(&mut host.reader, old_object, A, 10.0);
    host.reader.seek(2.0).unwrap();
    host.present();
    host.click(2);
    assert_eq!(host.reader.selected_pointer_target(), Some(replacement));
    effect(&mut host.reader, replacement_object, B, 20.0);
}

#[test]
fn prefix_browser_unavailable_seek_preserves_receipt_but_successful_seek_retires_it() {
    let (_, _, mut host) = Source::new();
    host.present();
    assert!(host.send(BrowserPointerKind::Press, 1).unwrap());
    let context = host.reader.publication_context();
    assert!(host.reader.seek(host.reader.frontier()).is_err());
    assert_eq!(host.reader.publication_context(), context);
    assert!(!host.presentation.needs_refresh(&host.reader));
    assert!(host.send(BrowserPointerKind::Release, 1).unwrap());
    host.reader.seek(0.5).unwrap();
    assert!(host.presentation.needs_refresh(&host.reader));
    assert!(!host.send(BrowserPointerKind::Press, 2).unwrap());
    assert!(host.reader.selected_pointer_target().is_none());
    assert!(!host.reader.interactions_active());
    host.present();
    host.click(3);
    assert!(host.reader.interactions_active());
}

#[test]
fn prefix_browser_consumed_unpresented_frame_cancels_without_replaying_release() {
    let (_, _, mut host) = Source::new();
    host.present();
    assert!(host.send(BrowserPointerKind::Press, 1).unwrap());
    let displayed = host.presentation.presented.clone();
    host.reader.advance_to(0.75).unwrap();
    host.reader.take_renderer_publication();
    assert!(!host.send(BrowserPointerKind::Release, 1).unwrap());
    assert_eq!(
        host.sequence, 2,
        "press and separate cancellation, not a release"
    );
    assert_eq!(host.presentation.presented, displayed);
    assert!(!host.reader.interactions_active());
    assert!(host.reader.selected_pointer_target().is_none());
    host.present();
    assert!(host.send(BrowserPointerKind::Release, 1).is_err());
    host.click(2);
    assert!(host.reader.interactions_active());
}

#[test]
fn prefix_browser_foreign_receipt_never_aliases_same_numeric_object_ids() {
    let (left_source, _, mut left) = Source::new();
    let (right_source, _, mut right) = Source::new();
    assert_eq!(left_source.node, right_source.node);
    left.present();
    right
        .presentation
        .did_present(left.presentation.presented.clone());
    let frame = right.reader.frame().clone();
    let publication = right.reader.publication_context();
    assert!(!right.send(BrowserPointerKind::Press, 1).unwrap());
    assert_eq!(right.sequence, 0);
    assert!(right.binding.is_none());
    assert_eq!(right.reader.frame(), &frame);
    assert_eq!(right.reader.publication_context(), publication);
    assert!(!right.reader.interactions_active());
    assert!(right.presentation.refresh_pending);
}

#[test]
fn prefix_browser_scroll_requires_presented_view_and_preserves_paused_effect() {
    let (_, _, mut host) = Source::new();
    let camera = host.reader.inspection_camera().unwrap();
    assert_eq!(
        host.presentation
            .scroll(
                &mut host.reader,
                &mut host.binding,
                1,
                SIZE,
                Vec2::new(400.0, 200.0),
                -100.0
            )
            .unwrap(),
        None
    );
    assert_eq!(host.reader.inspection_camera().unwrap(), camera);
    host.present();
    host.click(1);
    host.reader.advance_interactions(10.0).unwrap();
    host.reader.advance_interactions(10.125).unwrap();
    host.present();
    let time = host.reader.frame().time;
    let revision = host.reader.inspection_view_revision();
    assert_eq!(
        host.presentation
            .scroll(
                &mut host.reader,
                &mut host.binding,
                1,
                SIZE,
                Vec2::new(400.0, 200.0),
                -100.0
            )
            .unwrap(),
        Some(true)
    );
    assert!(host.reader.inspection_view_revision() > revision);
    assert!(host.reader.inspection_camera().unwrap().height < camera.height);
    assert!(host.reader.interactions_active());
    assert_eq!(host.reader.frame().time, time);
    assert!(host.presentation.presented.is_none());
    assert_eq!(
        host.presentation
            .scroll(
                &mut host.reader,
                &mut host.binding,
                1,
                SIZE,
                Vec2::new(400.0, 200.0),
                -100.0
            )
            .unwrap(),
        None
    );
    host.present();
    host.reader.advance_interactions(10.5).unwrap();
    assert!(!host.reader.interactions_active());
    assert_eq!(host.reader.frame().time, time);
}

#[test]
fn prefix_browser_hover_uses_current_presented_revision_and_causal_capture_fails_closed() {
    let (mut source, mut publisher, mut host) = Source::new();
    host.present();
    assert!(host.send(BrowserPointerKind::Move, 1).unwrap());
    assert_eq!(host.reader.hovered_pointer_target(), Some(source.node));
    let replacement = source.replace_and_finish(&mut publisher, &mut host.reader);
    assert_eq!(host.reader.hovered_pointer_target(), Some(source.node));
    host.reader.advance_to(1.5).unwrap();
    host.present();
    assert_eq!(host.reader.hovered_pointer_target(), Some(replacement));
    let frame = host.presentation.presented.clone().unwrap();
    let context = host.reader.publication_context();
    let sequence = host.sequence;
    let rejected = submit_captured_browser_pointer_input(
        &mut host.reader,
        &mut host.binding,
        &mut host.sequence,
        wire(BrowserPointerKind::Move, 1),
        &frame,
    );
    assert!(matches!(
        rejected,
        Err(BrowserPointerAdmissionError::Frame(
            PointerFrameError::Input(ExecutionSessionInputError::Interaction(_))
        ))
    ));
    assert_eq!(host.sequence, sequence);
    assert_eq!(host.reader.publication_context(), context);
}

#[test]
fn prefix_browser_worker_requires_render_ack_and_rejects_delayed_receipt() {
    let (source, _, mut host) = Source::new();
    let mut worker = WorkerPointerPresentation::default();
    worker
        .set_view(
            &mut host.reader,
            &mut host.binding,
            &mut host.sequence,
            PointerPresentationView {
                revision: 1,
                width: SIZE.x,
                height: SIZE.y,
            },
        )
        .unwrap();
    let frame = worker.capture(&host.reader).unwrap();
    host.reader.take_renderer_publication();
    worker.issue(7, 0, frame);
    let receipt = WorkerPointerReceipt {
        session: 7,
        sequence: 0,
        presentation: 1,
        view_revision: 1,
    };
    assert!(!worker
        .submit(
            &mut host.reader,
            &mut host.binding,
            &mut host.sequence,
            wire(BrowserPointerKind::Press, 1),
            Some(receipt)
        )
        .unwrap());
    assert_eq!(host.sequence, 0);
    assert!(worker.note_presented(&mut host.reader, receipt).unwrap());
    assert!(!worker
        .submit(
            &mut host.reader,
            &mut host.binding,
            &mut host.sequence,
            wire(BrowserPointerKind::Release, 1),
            Some(receipt)
        )
        .unwrap());
    for kind in [BrowserPointerKind::Press, BrowserPointerKind::Release] {
        assert!(worker
            .submit(
                &mut host.reader,
                &mut host.binding,
                &mut host.sequence,
                wire(kind, 2),
                Some(receipt)
            )
            .unwrap());
    }
    assert_eq!(host.reader.selected_pointer_target(), Some(source.node));
    host.reader.seek(0.75).unwrap();
    assert!(worker.needs_delta(&host.reader));
    let frame = worker.capture(&host.reader).unwrap();
    worker.issue(7, 1, frame);
    assert!(!worker.note_presented(&mut host.reader, receipt).unwrap());
    assert!(!worker
        .submit(
            &mut host.reader,
            &mut host.binding,
            &mut host.sequence,
            wire(BrowserPointerKind::Press, 3),
            Some(receipt)
        )
        .unwrap());
    let new_receipt = WorkerPointerReceipt {
        sequence: 1,
        presentation: 2,
        ..receipt
    };
    assert!(worker
        .note_presented(&mut host.reader, new_receipt)
        .unwrap());
    for kind in [BrowserPointerKind::Press, BrowserPointerKind::Release] {
        assert!(worker
            .submit(
                &mut host.reader,
                &mut host.binding,
                &mut host.sequence,
                wire(kind, 4),
                Some(new_receipt)
            )
            .unwrap());
    }
    assert!(host.reader.interactions_active());
}
