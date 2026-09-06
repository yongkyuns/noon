//! Canonical scalar tracker authoring over one shared semantic store.
//!
//! This module deliberately contains no reactive graph, execution signal IDs, or
//! timeline cursor. It only builds store-owned scalar input/derived declarations
//! and their deterministic track mutations; `ExecutionSession` owns evaluation.

use std::{cell::RefCell, rc::Rc};

use noon_core::{
    RateFunction, SemanticMutationTransaction, SemanticNodeCreation, SemanticNodeId,
    SemanticObjectProperty, SemanticSignalExpr, SemanticSignalSource, SemanticSignalValue,
    SemanticStore, SemanticVec3, TrackTiming,
};

use crate::{Mobject, Scene};

/// A scalar input signal in one canonical semantic store.
///
/// The handle owns neither its current value nor a timeline cursor. Both remain
/// authored/runtime state in the shared Rust pipeline.
#[derive(Clone, Debug)]
pub struct ValueTracker {
    store: Rc<RefCell<SemanticStore>>,
    node: SemanticNodeId,
}

impl ValueTracker {
    /// Create a scalar input in `store` without associating it with a Scene.
    ///
    /// This is the shared constructor for host-language values that can be
    /// created before their eventual Scene. The semantic store allocates the
    /// only identity and owns the scalar from construction onward.
    pub fn detached(store: Rc<RefCell<SemanticStore>>, initial: f64) -> Result<Self, String> {
        let node = store
            .borrow_mut()
            .insert_semantic_input_signal(initial)
            .map_err(|error| error.to_string())?;
        Ok(Self { store, node })
    }

    pub(crate) fn from_semantic_node(
        store: Rc<RefCell<SemanticStore>>,
        node: SemanticNodeId,
    ) -> Self {
        Self { store, node }
    }

    /// The scene-global semantic identity of this scalar input.
    pub const fn node_id(&self) -> SemanticNodeId {
        self.node
    }

    /// Whether this handle originates in `store`; callers still validate its
    /// generational node before using it.
    pub fn is_in_store(&self, store: &Rc<RefCell<SemanticStore>>) -> bool {
        Rc::ptr_eq(&self.store, store)
    }

    pub(crate) fn require_store(&self, store: &Rc<RefCell<SemanticStore>>) -> Result<(), String> {
        if !Rc::ptr_eq(&self.store, store) {
            return Err("ValueTracker belongs to another scene store".into());
        }
        self.store
            .borrow()
            .semantic_signal_state(self.node)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Read a tracker before it is associated with a Scene.
    pub fn detached_value(&self) -> Result<f64, String> {
        self.require_detached()?;
        tracker_track_endpoint(self)
    }

    /// Mutate a tracker before it is associated with a Scene.
    pub fn set_detached_value(&self, value: f64) -> Result<(), String> {
        self.require_detached()?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_signal(self.node, value);
        transaction
            .apply(&mut self.store.borrow_mut())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn require_detached(&self) -> Result<(), String> {
        self.require_store(&self.store)?;
        if self.store.borrow().has_semantic_signal_scope(self.node) {
            return Err("ValueTracker is already associated with a Scene".into());
        }
        Ok(())
    }
}

/// A canonical vector expression produced by `offset + tracker * direction`.
///
/// This handle contains only its derived semantic signal identity and store.
#[derive(Clone, Debug)]
pub struct TrackerPosition {
    store: Rc<RefCell<SemanticStore>>,
    node: SemanticNodeId,
}

impl TrackerPosition {
    pub const fn node_id(&self) -> SemanticNodeId {
        self.node
    }

    pub fn is_in_store(&self, store: &Rc<RefCell<SemanticStore>>) -> bool {
        Rc::ptr_eq(&self.store, store)
    }

    fn require_store(&self, store: &Rc<RefCell<SemanticStore>>) -> Result<(), String> {
        if !Rc::ptr_eq(&self.store, store) {
            return Err("tracker position belongs to another scene store".into());
        }
        self.store
            .borrow()
            .semantic_signal_state(self.node)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

/// One uncommitted deterministic scalar track declaration.
///
/// This configures only authored track semantics. `run_time` atomically adds the
/// declaration to the semantic store and then advances Scene's existing authoring
/// cursor; it never owns playback state.
pub struct ValueTrackerPlay<'a> {
    scene: &'a mut Scene,
    tracker: ValueTracker,
    target: f64,
    rate_function: RateFunction,
}

impl ValueTrackerPlay<'_> {
    pub fn rate_func(mut self, rate_function: RateFunction) -> Self {
        self.rate_function = rate_function;
        self
    }

    pub fn run_time(self, duration: f64) -> Result<(), String> {
        if !self.target.is_finite() {
            return Err("ValueTracker target must be finite".into());
        }
        if !duration.is_finite() || duration <= 0.0 {
            return Err("ValueTracker duration must be finite and positive".into());
        }
        let start = self.scene.time();
        if !(start + duration).is_finite() {
            return Err("ValueTracker track end time must be finite".into());
        }
        self.tracker.require_store(self.scene.store())?;
        if !self
            .scene
            .store()
            .borrow()
            .is_semantic_signal_scoped(self.scene.root(), self.tracker.node)
        {
            return Err("ValueTracker is not associated with this Scene".into());
        }
        let from = tracker_track_endpoint(&self.tracker)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_scalar_signal_track(
            self.tracker.node,
            from,
            self.target,
            TrackTiming::new(start, duration, self.rate_function),
        );
        transaction
            .apply(&mut self.scene.store().borrow_mut())
            .map_err(|error| error.to_string())?;
        // The same conditions were checked above; this cannot fail after the
        // declaration commits, avoiding a divergent facade-owned cursor.
        self.scene
            .wait(duration)
            .expect("prevalidated scalar track duration advances the scene cursor");
        Ok(())
    }
}

impl Scene {
    /// Create and scope a scalar input signal to this Scene in one semantic transaction.
    pub fn value_tracker(&self, initial: f64) -> Result<ValueTracker, String> {
        let creation =
            SemanticNodeCreation::input_signal(initial).map_err(|error| error.to_string())?;
        let mut transaction = SemanticMutationTransaction::new();
        let pending = transaction.create_node(creation);
        transaction.scope_signal(self.root(), pending);
        let result = transaction
            .apply(&mut self.store().borrow_mut())
            .map_err(|error| error.to_string())?;
        let node = result
            .resolve(pending)
            .expect("committed tracker creation resolves its transaction-local token");
        Ok(ValueTracker {
            store: Rc::clone(self.store()),
            node,
        })
    }

    /// Associate an existing detached signal with this Scene's execution scope.
    pub fn associate_value_tracker(&self, tracker: &ValueTracker) -> Result<(), String> {
        tracker.require_store(self.store())?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.scope_signal(self.root(), tracker.node_id());
        transaction
            .apply(&mut self.store().borrow_mut())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Build the first supported tracker expression, `offset + tracker * direction`.
    pub fn position_from_tracker(
        &self,
        tracker: &ValueTracker,
        direction: SemanticVec3,
        offset: SemanticVec3,
    ) -> Result<TrackerPosition, String> {
        tracker.require_store(self.store())?;
        if !direction.is_finite() || !offset.is_finite() {
            return Err("tracker direction and offset must be finite".into());
        }
        let expression = SemanticSignalExpr::Add(
            Box::new(SemanticSignalExpr::Constant(SemanticSignalValue::Vec3(
                offset,
            ))),
            Box::new(SemanticSignalExpr::Mul(
                Box::new(SemanticSignalExpr::signal(tracker.node)),
                Box::new(SemanticSignalExpr::Constant(SemanticSignalValue::Vec3(
                    direction,
                ))),
            )),
        );
        let node = self
            .store()
            .borrow_mut()
            .insert_semantic_derived_signal(expression)
            .map_err(|error| error.to_string())?;
        Ok(TrackerPosition {
            store: Rc::clone(self.store()),
            node,
        })
    }

    /// Bind a tracker-derived position through the shared semantic subscription.
    pub fn bind_position(
        &self,
        object: &Mobject,
        position: &TrackerPosition,
    ) -> Result<(), String> {
        self.require_object(object)?;
        position.require_store(self.store())?;
        self.store()
            .borrow_mut()
            .bind_semantic_signal(
                position.node,
                object.node_id(),
                SemanticObjectProperty::Translation,
            )
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Set a non-timeline-owned tracker through the shared mutation transaction.
    pub fn set_value(&self, tracker: &ValueTracker, value: f64) -> Result<(), String> {
        tracker.require_store(self.store())?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_signal(tracker.node, value);
        transaction
            .apply(&mut self.store().borrow_mut())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Read the scalar track value at this Scene's shared authored cursor.
    ///
    /// The semantic store selects and eases its own declarations. This facade
    /// does not retain a value, evaluate an expression, or interpolate in a
    /// language wrapper.
    pub fn value_tracker_value(&self, tracker: &ValueTracker) -> Result<f64, String> {
        tracker.require_store(self.store())?;
        self.store()
            .borrow()
            .semantic_input_scalar_value_at(tracker.node, self.time())
            .map_err(|error| error.to_string())
    }

    /// Begin one canonical deterministic scalar-track declaration.
    pub fn play_value(&mut self, tracker: &ValueTracker, target: f64) -> ValueTrackerPlay<'_> {
        ValueTrackerPlay {
            scene: self,
            tracker: tracker.clone(),
            target,
            rate_function: RateFunction::Smooth,
        }
    }
}

fn tracker_track_endpoint(tracker: &ValueTracker) -> Result<f64, String> {
    let store = tracker.store.borrow();
    let state = store
        .semantic_signal_state(tracker.node)
        .map_err(|error| error.to_string())?;
    if let Some(entry) = state.scalar_timeline().last() {
        return Ok(entry.terminal_value());
    }
    match state.source() {
        SemanticSignalSource::Input(SemanticSignalValue::Scalar(value)) => Ok(*value),
        SemanticSignalSource::Input(_) => Err("ValueTracker signal is not scalar".into()),
        SemanticSignalSource::Derived(_) => Err("ValueTracker signal is derived".into()),
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{RateFunction, SemanticVec3};

    use super::*;

    #[test]
    fn tracker_declaration_uses_one_store_cursor_and_scalar_track() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        scene.add(&circle).unwrap();
        let tracker = scene.value_tracker(0.0).unwrap();
        let position = scene
            .position_from_tracker(
                &tracker,
                SemanticVec3::new(1.0, 0.0, 0.0),
                SemanticVec3::new(-2.0, 0.0, 0.0),
            )
            .unwrap();
        scene.bind_position(&circle, &position).unwrap();

        scene
            .play_value(&tracker, 4.0)
            .rate_func(RateFunction::Linear)
            .run_time(2.0)
            .unwrap();

        assert_eq!(scene.time(), 2.0);
        let signal = scene
            .store()
            .borrow()
            .semantic_signal_state(tracker.node_id())
            .unwrap()
            .clone();
        let [noon_core::SemanticScalarSignalTimelineEntry::Track(track)] = signal.scalar_timeline()
        else {
            panic!("expected one scalar track")
        };
        assert_eq!(track.from(), 0.0);
        assert_eq!(track.to(), 4.0);
        assert_eq!(track.timing().start_time, 0.0);
        assert_eq!(track.timing().duration, 2.0);
        assert_eq!(track.timing().easing, RateFunction::Linear);
        assert_eq!(scene.value_tracker_value(&tracker).unwrap(), 4.0);
        assert!(scene.set_value(&tracker, 1.0).is_err());
    }

    #[test]
    fn scoped_tracker_lowers_before_first_play_and_live_creation_enrolls_one_input() {
        let scene = Scene::new();
        let tracker = scene.value_tracker(1.5).unwrap();
        let session = scene.execution_session().unwrap();
        assert_eq!(
            session.effective_signal_value(tracker.node_id()),
            Some(&noon_core::ReactiveValue::Scalar(1.5))
        );

        let mut session = scene.execution_session().unwrap();
        let before = session.publication_context();
        let live_tracker = scene.live(&mut session).value_tracker(2.25).unwrap();
        assert_eq!(
            session.effective_signal_value(live_tracker.node_id()),
            Some(&noon_core::ReactiveValue::Scalar(2.25))
        );
        assert_eq!(session.frame().objects.len(), 0);
        assert_eq!(
            session.publication_context().scene_revision().get(),
            before.scene_revision().get() + 1
        );
        assert!(scene
            .store()
            .borrow()
            .semantic_scoped_signals(scene.root())
            .unwrap()
            .contains(&live_tracker.node_id()));
    }

    #[test]
    fn live_tracker_creation_failure_leaves_store_and_runtime_unchanged() {
        let scene = Scene::new();
        let mut session = scene.execution_session().unwrap();
        let revision = scene.store().borrow().scene_revision();
        let publication = session.publication_context();
        let frame = session.frame().clone();
        assert!(scene.live(&mut session).value_tracker(f64::MAX).is_err());
        assert_eq!(scene.store().borrow().scene_revision(), revision);
        assert_eq!(session.publication_context(), publication);
        assert_eq!(session.frame(), &frame);
    }

    #[test]
    fn existing_detached_tracker_requires_explicit_live_association() {
        let scene = Scene::new();
        let detached = scene
            .store()
            .borrow_mut()
            .insert_semantic_input_signal(4.0_f64)
            .unwrap();
        let tracker = ValueTracker::from_semantic_node(Rc::clone(scene.store()), detached);
        let mut session = scene.execution_session().unwrap();
        assert!(session.effective_signal_value(detached).is_none());
        assert!(
            scene
                .live(&mut session)
                .declare_and_activate_value_tracker(
                    &tracker,
                    5.0,
                    1.0,
                    noon_core::RateFunction::Linear,
                )
                .is_err()
        );

        scene
            .live(&mut session)
            .associate_value_tracker(&tracker)
            .unwrap();
        assert_eq!(
            session.effective_signal_value(detached),
            Some(&noon_core::ReactiveValue::Scalar(4.0))
        );
    }

    #[test]
    fn detached_tracker_value_stays_store_owned_before_scene_association() {
        let scene = Scene::new();
        let tracker = ValueTracker::detached(Rc::clone(scene.store()), 1.25).unwrap();

        assert_eq!(tracker.detached_value().unwrap(), 1.25);
        tracker.set_detached_value(2.5).unwrap();
        assert_eq!(tracker.detached_value().unwrap(), 2.5);

        scene.associate_value_tracker(&tracker).unwrap();
        assert!(tracker.detached_value().is_err());
        assert!(tracker.set_detached_value(3.0).is_err());
        assert_eq!(scene.value_tracker_value(&tracker).unwrap(), 2.5);
    }

    #[test]
    fn foreign_scene_rejects_detached_tracker_without_scoping_it() {
        let scene = Scene::new();
        let foreign = Scene::new();
        let tracker = ValueTracker::detached(Rc::clone(scene.store()), 1.0).unwrap();

        assert!(foreign.associate_value_tracker(&tracker).is_err());
        assert_eq!(tracker.detached_value().unwrap(), 1.0);
        assert!(!scene
            .store()
            .borrow()
            .is_semantic_signal_scoped(scene.root(), tracker.node_id()));
    }

    #[test]
    fn invalid_detached_tracker_association_rolls_back_scope_and_runtime() {
        let scene = Scene::new();
        let detached = scene
            .store()
            .borrow_mut()
            .insert_semantic_input_signal(f64::MAX)
            .unwrap();
        let tracker = ValueTracker::from_semantic_node(Rc::clone(scene.store()), detached);
        let mut session = scene.execution_session().unwrap();
        let revision = scene.store().borrow().scene_revision();
        let publication = session.publication_context();
        let frame = session.frame().clone();

        assert!(scene
            .live(&mut session)
            .associate_value_tracker(&tracker)
            .is_err());

        let store = scene.store().borrow();
        assert_eq!(store.scene_revision(), revision);
        assert!(!store
            .semantic_scoped_signals(scene.root())
            .unwrap()
            .contains(&detached));
        assert_eq!(session.publication_context(), publication);
        assert_eq!(session.frame(), &frame);
        assert!(session.effective_signal_value(detached).is_none());
    }

    #[test]
    fn tracker_handles_reject_foreign_scene_stores() {
        let scene = Scene::new();
        let foreign = Scene::new();
        let tracker = scene.value_tracker(0.0).unwrap();
        assert!(foreign
            .position_from_tracker(
                &tracker,
                SemanticVec3::new(1.0, 0.0, 0.0),
                SemanticVec3::ZERO,
            )
            .is_err());
    }

    #[test]
    fn tracker_wait_and_next_track_share_the_rust_authored_cursor() {
        let mut scene = Scene::new();
        let tracker = scene.value_tracker(0.0).unwrap();
        scene
            .play_value(&tracker, 4.0)
            .rate_func(RateFunction::Linear)
            .run_time(2.0)
            .unwrap();
        scene.wait(1.0).unwrap();
        scene
            .play_value(&tracker, 6.0)
            .rate_func(RateFunction::Linear)
            .run_time(1.0)
            .unwrap();

        let store = scene.store().borrow();
        let timeline = store
            .semantic_signal_state(tracker.node_id())
            .unwrap()
            .scalar_timeline();
        assert_eq!(timeline.len(), 2);
        let noon_core::SemanticScalarSignalTimelineEntry::Track(second) = timeline[1] else {
            panic!("expected a second scalar track")
        };
        assert_eq!(second.from(), 4.0);
        assert_eq!(second.to(), 6.0);
        assert_eq!(second.timing().start_time, 3.0);
        assert_eq!(scene.time(), 4.0);
        assert_eq!(scene.value_tracker_value(&tracker).unwrap(), 6.0);
    }
}
