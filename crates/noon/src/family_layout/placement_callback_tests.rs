//! Source-side placement after a committed updater, not an export-only path.
use std::{cell::RefCell, rc::Rc};

use crate::{
    AnimationOptions, AuthoringError, ExecutionSession, LiveLayoutTarget, LiveSessionError,
    Mobject, RateFunction, RustHostCallbackTable, Scene, Transform2D, UnsupportedAuthoringOperation,
    Vec2,
};
use noon_core::{HostCallbackId, SemanticObjectProperty, SemanticVec3};

const UPDATER: HostCallbackId = HostCallbackId::new(189_625);

struct Fixture {
    scene: Scene,
    marker: Mobject,
    session: ExecutionSession,
    callbacks: RustHostCallbackTable,
    observed: Rc<RefCell<Vec<Transform2D>>>,
}

impl Fixture {
    fn new(affine: bool, reactive: bool) -> Self {
        let mut scene = Scene::new();
        let marker = scene.square(1.0).unwrap();
        let neighbor = scene.square(0.5).unwrap();
        scene.add(&marker).unwrap();
        scene.add(&neighbor).unwrap();
        let observed = Rc::new(RefCell::new(Vec::new()));
        let trace = Rc::clone(&observed);
        let mut callbacks = RustHostCallbackTable::new();
        callbacks
            .insert(UPDATER, move |context| {
                let mut transform = context.target_state().transform;
                trace.borrow_mut().push(transform);
                transform.translation.x += 0.25;
                transform.translation.y += 0.5;
                if affine {
                    transform.rotation = 0.25;
                    transform.scale = Vec2::new(2.0, 0.5);
                }
                context.set_target_transform(transform)
            })
            .unwrap();
        {
            let mut store = scene.integration_store().borrow_mut();
            if reactive {
                let position = store
                    .insert_semantic_input_signal(SemanticVec3::new(2.0, 0.0, 0.0))
                    .unwrap();
                store
                    .bind_semantic_signal(
                        position,
                        marker.node_id(),
                        SemanticObjectProperty::Translation,
                    )
                    .unwrap();
            }
            callbacks
                .add_updater(&mut store, marker.node_id(), UPDATER, 0.0, None)
                .unwrap();
        }
        let session = scene.execution_session().unwrap();
        Self {
            scene,
            marker,
            session,
            callbacks,
            observed,
        }
    }

    fn finish_wait(&mut self) {
        let segment = self
            .scene
            .live(&mut self.session)
            .wait_segment(0.125)
            .unwrap();
        self.callbacks
            .advance_segment_to(&mut self.session, segment, segment.end_time())
            .unwrap();
        self.scene
            .live(&mut self.session)
            .complete_segment(segment)
            .unwrap();
        drop(self.session.take_renderer_publication());
    }

    fn transform(&self) -> Transform2D {
        self.session.frame().objects[0].transform
    }
}

#[test]
fn move_to_after_updater_preserves_affine_state_and_registration_in_one_publication() {
    let mut fixture = Fixture::new(true, false);
    fixture.finish_wait();
    let before = fixture.session.publication_context();
    let time = fixture.session.frame().time;
    let count = fixture.observed.borrow().len();
    let authored = fixture.marker.state().unwrap();
    let effective = fixture.transform();
    let registrations = fixture
        .scene
        .integration_store()
        .borrow()
        .semantic_updater_registrations(fixture.marker.node_id())
        .unwrap()
        .to_vec();
    let result = fixture
        .scene
        .live(&mut fixture.session)
        .move_to_point(&fixture.marker, 3.0, -2.0)
        .unwrap();
    assert_eq!(result.impacts().len(), 1);
    assert_eq!(fixture.transform().translation, Vec2::new(3.0, -2.0));
    assert_eq!(fixture.transform().rotation, effective.rotation);
    assert_eq!(fixture.transform().scale, effective.scale);
    let after = fixture.marker.state().unwrap();
    assert_eq!(after.transform.scale, authored.transform.scale);
    assert_eq!(after.transform.orientation, authored.transform.orientation);
    assert_eq!(fixture.session.frame().time, time);
    assert_eq!(fixture.observed.borrow().len(), count);
    assert_eq!(
        fixture.session.publication_context().frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
    assert_eq!(fixture.session.take_frame_changes().object_indices(), &[0]);
    assert_eq!(
        fixture
            .scene
            .integration_store()
            .borrow()
            .semantic_updater_registrations(fixture.marker.node_id())
            .unwrap(),
        registrations.as_slice()
    );

    // Repeated placement at the same logical time retains a current receipt.
    fixture
        .scene
        .live(&mut fixture.session)
        .move_to_point(&fixture.marker, -3.0, 2.0)
        .unwrap();
    assert_eq!(fixture.transform().translation, Vec2::new(-3.0, 2.0));
    assert_eq!(fixture.transform().rotation, effective.rotation);
    let expected = fixture.transform();
    fixture
        .callbacks
        .advance_to(&mut fixture.session, time)
        .unwrap();
    assert_eq!(fixture.observed.borrow().len(), count);
    let segment = fixture
        .scene
        .live(&mut fixture.session)
        .wait_segment(0.125)
        .unwrap();
    fixture
        .callbacks
        .advance_segment_to(&mut fixture.session, segment, segment.end_time())
        .unwrap();
    assert_eq!(fixture.observed.borrow()[count], expected);
}

#[test]
fn callback_position_can_reset_to_the_unchanged_authored_translation() {
    let mut fixture = Fixture::new(false, false);
    fixture.finish_wait();
    let before = fixture.session.publication_context();
    assert_ne!(fixture.transform().translation, Vec2::ZERO);
    let authored = fixture.marker.state().unwrap();
    fixture
        .scene
        .live(&mut fixture.session)
        .move_to_point(&fixture.marker, 0.0, 0.0)
        .unwrap();
    assert_eq!(fixture.marker.state().unwrap(), authored);
    assert_eq!(fixture.transform().translation, Vec2::ZERO);
    assert_eq!(
        fixture.session.publication_context().scene_revision(),
        before.scene_revision()
    );
    assert_eq!(
        fixture.session.publication_context().frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
}

#[test]
fn callback_move_to_keeps_masked_axes_and_rejects_invalid_targets_atomically() {
    let mut fixture = Fixture::new(false, false);
    fixture.finish_wait();
    let before = fixture.session.publication_context();
    let authored = fixture.marker.state().unwrap();
    let effective = fixture.transform();
    let foreign = Scene::new().square(0.5).unwrap();
    assert!(fixture
        .scene
        .live(&mut fixture.session)
        .move_to(
            &fixture.marker,
            LiveLayoutTarget::Mobject(&foreign),
            (0.0, 0.0),
            (1.0, 1.0),
        )
        .is_err());
    assert!(fixture
        .scene
        .live(&mut fixture.session)
        .move_to(
            &fixture.marker,
            LiveLayoutTarget::Point(1.0, 2.0),
            (0.0, 0.0),
            (f64::NAN, 1.0),
        )
        .is_err());
    assert_eq!(fixture.session.publication_context(), before);
    assert_eq!(fixture.marker.state().unwrap(), authored);
    assert_eq!(fixture.transform(), effective);
    fixture
        .scene
        .live(&mut fixture.session)
        .move_to(
            &fixture.marker,
            LiveLayoutTarget::Point(4.0, 99.0),
            (0.0, 0.0),
            (1.0, 0.0),
        )
        .unwrap();
    assert_eq!(
        fixture.transform().translation,
        Vec2::new(4.0, effective.translation.y)
    );
}

#[test]
fn active_animation_and_reactive_affine_drivers_are_not_callback_placement() {
    let mut fixture = Fixture::new(false, true);
    fixture.finish_wait();
    let before = fixture.session.publication_context();
    assert!(matches!(
        fixture
            .scene
            .live(&mut fixture.session)
            .move_to_point(&fixture.marker, 9.0, 0.0),
        Err(LiveSessionError::Authoring(AuthoringError::Unsupported(
            UnsupportedAuthoringOperation::PlacementEffectiveAffineDriver
        )))
    ));
    assert_eq!(fixture.session.publication_context(), before);

    let mut fixture = Fixture::new(false, false);
    let mut target = fixture.marker.target_editor().unwrap();
    target.set_translation(4.0, 0.0).unwrap();
    let segment = fixture
        .scene
        .live(&mut fixture.session)
        .declare_and_activate_transform_to(
            &fixture.marker,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    fixture
        .callbacks
        .advance_segment_to(&mut fixture.session, segment, 0.5)
        .unwrap();
    let before = fixture.session.publication_context();
    assert!(fixture
        .scene
        .live(&mut fixture.session)
        .move_to_point(&fixture.marker, 9.0, 0.0)
        .is_err());
    assert_eq!(fixture.session.publication_context(), before);
}

#[test]
fn placement_cannot_consume_an_unpublished_callback_or_its_writes() {
    let mut fixture = Fixture::new(false, false);
    fixture.finish_wait();
    let segment = fixture
        .scene
        .live(&mut fixture.session)
        .wait_segment(0.125)
        .unwrap();
    let before = fixture.session.publication_context();
    let effective = fixture.transform();
    let authored = fixture.marker.state().unwrap();
    let count = fixture.observed.borrow().len();
    assert!(matches!(
        fixture
            .session
            .advance_segment_to_callback_barrier(segment, segment.end_time())
            .unwrap(),
        crate::integration::CallbackAdvance::HostRequired { .. }
    ));
    let token = fixture.session.pending_callback_token();
    assert!(fixture
        .scene
        .live(&mut fixture.session)
        .move_to_point(&fixture.marker, 9.0, 0.0)
        .is_err());
    assert_eq!(fixture.session.pending_callback_token(), token);
    assert_eq!(fixture.session.publication_context(), before);
    assert_eq!(fixture.transform(), effective);
    assert_eq!(fixture.marker.state().unwrap(), authored);
    assert_eq!(fixture.observed.borrow().len(), count);
}
