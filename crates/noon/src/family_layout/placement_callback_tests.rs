//! Source-side placement after a committed updater, not an export-only path.
use std::{cell::RefCell, rc::Rc};

use crate::{
    AnimationOptions, AuthoringError, LiveLayoutTarget, Mobject, RateFunction,
    RustHostCallbackTable, Scene, Transform2D, UnsupportedAuthoringOperation, Vec2,
};
use noon_core::{HostCallbackId, SemanticObjectProperty, SemanticVec3};

const UPDATER: HostCallbackId = HostCallbackId::new(189_625);

struct Fixture {
    source: crate::SceneExecution,
    marker: Mobject,
    callbacks: RustHostCallbackTable,
    observed: Rc<RefCell<Vec<Transform2D>>>,
}

impl Fixture {
    fn new(affine: bool, reactive: bool) -> Self {
        Self::with_grid_line(affine, reactive, None)
    }

    fn with_grid_line(affine: bool, reactive: bool, horizontal: Option<bool>) -> Self {
        let mut scene = Scene::new();
        let marker = if let Some(horizontal) = horizontal {
            let plane = scene
                .number_plane(&crate::ManimNumberPlaneOptions::default())
                .unwrap();
            let background = plane.background_lines().unwrap();
            let index = plane.grid_line_indices(horizontal).unwrap()[0] as usize;
            let id = scene
                .integration_store()
                .borrow()
                .semantic_family_members_checked(background.node_id())
                .unwrap()[index];
            Mobject::from_node(Rc::clone(scene.integration_store()), id).unwrap()
        } else {
            scene.square(1.0).unwrap()
        };
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
        let source = scene.into_execution().unwrap();
        Self {
            source,
            marker,
            callbacks,
            observed,
        }
    }

    fn finish_wait(&mut self) {
        let segment = self.source.live().wait_segment(0.125).unwrap();
        self.callbacks
            .advance_segment_to(self.source.session_mut(), segment, segment.end_time())
            .unwrap();
        self.source.live().complete_segment(segment).unwrap();
        drop(self.source.session_mut().take_renderer_publication());
    }

    fn transform(&self) -> Transform2D {
        self.source.session().frame().objects[0].transform
    }
}

#[test]
fn grid_line_callback_placements_preserve_settled_affine_ownership() {
    for horizontal in [true, false] {
        let mut fixture = Fixture::with_grid_line(true, false, Some(horizontal));
        fixture.finish_wait();
        let time = fixture.source.session().frame().time;
        let count = fixture.observed.borrow().len();
        let effective = fixture.transform();
        for (x, y) in [(3.0, -2.0), (-3.0, 2.0)] {
            fixture
                .source
                .scene_mut()
                .move_to_point(&fixture.marker, x, y)
                .unwrap();
            assert_eq!(
                fixture.transform().translation,
                Vec2::new(x as f32, y as f32)
            );
            assert_eq!(fixture.transform().rotation, effective.rotation);
            assert_eq!(fixture.transform().scale, effective.scale);
            assert_eq!(fixture.source.session().frame().time, time);
            assert_eq!(fixture.observed.borrow().len(), count);
            assert_eq!(
                fixture.marker.state().unwrap().role(),
                noon_core::SemanticObjectRole::NumberPlaneGridLine { horizontal }
            );
        }
    }
}

#[test]
fn move_to_after_updater_preserves_affine_state_and_registration_in_one_publication() {
    let mut fixture = Fixture::new(true, false);
    fixture.finish_wait();
    let before = fixture.source.session().publication_context();
    let time = fixture.source.session().frame().time;
    let count = fixture.observed.borrow().len();
    let authored = fixture.marker.state().unwrap();
    let effective = fixture.transform();
    let registrations = fixture
        .source
        .scene()
        .integration_store()
        .borrow()
        .semantic_updater_registrations(fixture.marker.node_id())
        .unwrap()
        .to_vec();
    let result = fixture
        .source
        .scene_mut()
        .move_to_point(&fixture.marker, 3.0, -2.0)
        .unwrap();
    assert_eq!(result.impacts().len(), 1);
    assert_eq!(fixture.transform().translation, Vec2::new(3.0, -2.0));
    assert_eq!(fixture.transform().rotation, effective.rotation);
    assert_eq!(fixture.transform().scale, effective.scale);
    let after = fixture.marker.state().unwrap();
    assert_eq!(after.transform.scale, authored.transform.scale);
    assert_eq!(after.transform.orientation, authored.transform.orientation);
    assert_eq!(fixture.source.session().frame().time, time);
    assert_eq!(fixture.observed.borrow().len(), count);
    assert_eq!(
        fixture.source.session().publication_context().frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
    assert_eq!(
        fixture
            .source
            .session_mut()
            .take_frame_changes()
            .object_indices(),
        &[0]
    );
    assert_eq!(
        fixture
            .source
            .scene()
            .integration_store()
            .borrow()
            .semantic_updater_registrations(fixture.marker.node_id())
            .unwrap(),
        registrations.as_slice()
    );

    // Repeated placement at the same logical time retains a current receipt.
    fixture
        .source
        .scene_mut()
        .move_to_point(&fixture.marker, -3.0, 2.0)
        .unwrap();
    assert_eq!(fixture.transform().translation, Vec2::new(-3.0, 2.0));
    assert_eq!(fixture.transform().rotation, effective.rotation);
    let expected = fixture.transform();
    fixture
        .callbacks
        .advance_to(fixture.source.session_mut(), time)
        .unwrap();
    assert_eq!(fixture.observed.borrow().len(), count);
    let segment = fixture.source.live().wait_segment(0.125).unwrap();
    fixture
        .callbacks
        .advance_segment_to(fixture.source.session_mut(), segment, segment.end_time())
        .unwrap();
    assert_eq!(fixture.observed.borrow()[count], expected);
}

#[test]
fn callback_position_can_reset_to_the_unchanged_authored_translation() {
    let mut fixture = Fixture::new(false, false);
    fixture.finish_wait();
    let before = fixture.source.session().publication_context();
    assert_ne!(fixture.transform().translation, Vec2::ZERO);
    let authored = fixture.marker.state().unwrap();
    fixture
        .source
        .scene_mut()
        .move_to_point(&fixture.marker, 0.0, 0.0)
        .unwrap();
    assert_eq!(fixture.marker.state().unwrap(), authored);
    assert_eq!(fixture.transform().translation, Vec2::ZERO);
    assert_eq!(
        fixture
            .source
            .session()
            .publication_context()
            .scene_revision(),
        before.scene_revision()
    );
    assert_eq!(
        fixture.source.session().publication_context().frame_epoch(),
        before.frame_epoch().checked_next().unwrap()
    );
}

#[test]
fn callback_move_to_keeps_masked_axes_and_rejects_invalid_targets_atomically() {
    let mut fixture = Fixture::new(false, false);
    fixture.finish_wait();
    let before = fixture.source.session().publication_context();
    let authored = fixture.marker.state().unwrap();
    let effective = fixture.transform();
    let Fixture {
        mut source, marker, ..
    } = fixture;
    let scene = source.scene_mut();
    let foreign = Scene::new().square(0.5).unwrap();
    assert!(scene
        .move_to(
            &marker,
            LiveLayoutTarget::Mobject(&foreign),
            (0.0, 0.0),
            (1.0, 1.0),
        )
        .is_err());
    assert!(scene
        .move_to(
            &marker,
            LiveLayoutTarget::Point(1.0, 2.0),
            (0.0, 0.0),
            (f64::NAN, 1.0),
        )
        .is_err());
    assert_eq!(scene.owned_execution().publication_context(), before);
    assert_eq!(marker.state().unwrap(), authored);
    assert_eq!(scene.effective(&marker).unwrap().transform, effective);
    scene
        .move_to(
            &marker,
            LiveLayoutTarget::Point(4.0, 99.0),
            (0.0, 0.0),
            (1.0, 0.0),
        )
        .unwrap();
    assert_eq!(
        scene.effective(&marker).unwrap().transform.translation,
        Vec2::new(4.0, effective.translation.y)
    );
}

#[test]
fn active_animation_and_reactive_affine_drivers_are_not_callback_placement() {
    let mut fixture = Fixture::new(false, true);
    fixture.finish_wait();
    let before = fixture.source.session().publication_context();
    assert!(matches!(
        fixture
            .source
            .scene_mut()
            .move_to_point(&fixture.marker, 9.0, 0.0),
        Err(AuthoringError::Unsupported(
            UnsupportedAuthoringOperation::PlacementEffectiveAffineDriver
        ))
    ));
    assert_eq!(fixture.source.session().publication_context(), before);

    let mut fixture = Fixture::new(false, false);
    let target = fixture
        .source
        .scene_mut()
        .target_editor(&fixture.marker)
        .unwrap();
    fixture
        .source
        .scene_mut()
        .set_translation(&target, 4.0, 0.0)
        .unwrap();
    let segment = fixture
        .source
        .live()
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
        .advance_segment_to(fixture.source.session_mut(), segment, 0.5)
        .unwrap();
    let before = fixture.source.session().publication_context();
    assert!(fixture
        .source
        .scene_mut()
        .move_to_point(&fixture.marker, 9.0, 0.0)
        .is_err());
    assert_eq!(fixture.source.session().publication_context(), before);
}

#[test]
fn placement_cannot_consume_an_unpublished_callback_or_its_writes() {
    let mut fixture = Fixture::new(false, false);
    fixture.finish_wait();
    let segment = fixture.source.live().wait_segment(0.125).unwrap();
    let before = fixture.source.session().publication_context();
    let effective = fixture.transform();
    let authored = fixture.marker.state().unwrap();
    let count = fixture.observed.borrow().len();
    assert!(matches!(
        fixture
            .source
            .session_mut()
            .advance_segment_to_callback_barrier(segment, segment.end_time())
            .unwrap(),
        crate::integration::CallbackAdvance::HostRequired { .. }
    ));
    let token = fixture.source.session().pending_callback_token();
    assert!(fixture
        .source
        .scene_mut()
        .move_to_point(&fixture.marker, 9.0, 0.0)
        .is_err());
    assert_eq!(fixture.source.session().pending_callback_token(), token);
    assert_eq!(fixture.source.session().publication_context(), before);
    assert_eq!(fixture.transform(), effective);
    assert_eq!(fixture.marker.state().unwrap(), authored);
    assert_eq!(fixture.observed.borrow().len(), count);
}

#[test]
fn consecutive_live_placements_preserve_one_completed_callback_phase() {
    use crate::execution_session::{CallbackAdvance, EffectiveSemanticPropertyWrite};
    use noon_core::{HostCallbackId, SemanticMutationTransaction, Vec2};

    let mut scene = Scene::new();
    let marker = scene.square(0.75).unwrap();
    scene.add(&marker).unwrap();
    let mut register = SemanticMutationTransaction::new();
    register.add_updater(marker.node_id(), HostCallbackId::new(1), 0.0, None);
    register.add_updater(marker.node_id(), HostCallbackId::new(2), 0.0, None);
    register
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);

    // Two ordered updater callbacks are admitted at the original time.
    let zero = match scene
        .owned_execution_mut()
        .advance_to_callback_barrier(0.0)
        .unwrap()
    {
        CallbackAdvance::HostRequired {
            invocations,
            overlay,
        } => {
            assert_eq!(invocations.len(), 2);
            overlay
        }
        CallbackAdvance::Ready(_) => panic!("time-zero callbacks must run"),
    };
    scene
        .owned_execution_mut()
        .commit_required_callback_phase(zero.finish())
        .unwrap();
    let mut settled = match scene
        .owned_execution_mut()
        .advance_to_callback_barrier(0.25)
        .unwrap()
    {
        CallbackAdvance::HostRequired {
            invocations,
            overlay,
        } => {
            assert_eq!(invocations.len(), 2);
            assert_eq!(overlay.delta_time(), 0.25);
            overlay
        }
        CallbackAdvance::Ready(_) => panic!("quarter-second callbacks must run"),
    };
    settled
        .write(EffectiveSemanticPropertyWrite::Translation {
            object: marker.node_id(),
            translation: Vec2::new(0.25, 0.25),
        })
        .unwrap();
    scene
        .owned_execution_mut()
        .commit_required_callback_phase(settled.finish())
        .unwrap();

    // First move_to resets an effective callback-owned position to the authored
    // origin. The second uses the normal authored translation path at the same
    // time; neither may invent a new zero-dt updater invocation.
    for (x, y) in [(0.0, 0.0), (0.5, 1.0)] {
        scene
            .move_to(
                &marker,
                LiveLayoutTarget::Point(x, y),
                (0.0, 0.0),
                (1.0, 1.0),
            )
            .unwrap();
    }
    assert_eq!(scene.owned_execution().frame().time, 0.25);
    assert!(matches!(
        scene
            .owned_execution_mut()
            .advance_to_callback_barrier(0.25)
            .unwrap(),
        CallbackAdvance::Ready(_)
    ));

    // Deliberately changing the callback plan at the same authored instant
    // must invalidate the receipt and allow the newly registered occurrence.
    let mut extra = SemanticMutationTransaction::new();
    extra.add_updater(marker.node_id(), HostCallbackId::new(3), 0.25, None);
    scene.apply_semantic_transaction(extra).unwrap();
    let changed = match scene
        .owned_execution_mut()
        .advance_to_callback_barrier(0.25)
        .unwrap()
    {
        CallbackAdvance::HostRequired {
            invocations,
            overlay,
        } => {
            assert_eq!(invocations.len(), 3);
            assert_eq!(overlay.delta_time(), 0.0);
            overlay
        }
        CallbackAdvance::Ready(_) => panic!("new callback must run at its activation"),
    };
    scene
        .owned_execution_mut()
        .commit_required_callback_phase(changed.finish())
        .unwrap();

    let next = match scene
        .owned_execution_mut()
        .advance_to_callback_barrier(0.5)
        .unwrap()
    {
        CallbackAdvance::HostRequired {
            invocations,
            overlay,
        } => {
            assert_eq!(invocations.len(), 3);
            assert_eq!(overlay.delta_time(), 0.25);
            overlay
        }
        CallbackAdvance::Ready(_) => panic!("later callback time must still run"),
    };
    scene
        .owned_execution_mut()
        .commit_required_callback_phase(next.finish())
        .unwrap();
}
