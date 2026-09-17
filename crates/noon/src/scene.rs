//! Direct authoring scope over one family in the shared semantic store.
mod image;
use crate::{
    AuthoringError, ExecutionSession, LiveSession, Mobject, MobjectFamily, MobjectTarget,
    SceneMembershipRequest, ValueTracker,
};
use noon_core::{
    AnimationOptions, GeometryRef, RateFunction, SemanticMutationImpact,
    SemanticMutationTransaction, SemanticMutationTransactionResult, SemanticNodeCreation,
    SemanticNodeId, SemanticStore, SemanticStyle, VectorPath,
};
use std::{cell::RefCell, rc::Rc};

pub(crate) fn publish_geometry_options(
    options: crate::ManimGeometryOptions,
    store: &mut SemanticStore,
    publish: impl FnOnce(
        &mut SemanticStore,
        SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError>,
) -> Result<SemanticNodeId, AuthoringError> {
    options.with_state(store, |store, state| {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_node(SemanticNodeCreation::object(state));
        let result = publish(store, transaction)?;
        let [SemanticMutationImpact::NodeAdded { node }] = result.impacts() else {
            unreachable!("one detached geometry creation has one exact semantic impact")
        };
        Ok(*node)
    })
}

/// A scene owns its shared semantic store/root and, after bootstrap, the one
/// execution component lowered from them. The optional execution slot is control
/// ownership only; Runtime state remains owned by the contained [`ExecutionSession`].
/// Direct integration callers may still create standalone sessions during migration.
#[derive(Debug)]
pub struct Scene {
    store: Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    cursor: f64,
    execution: Option<ExecutionSession>,
}
impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}
impl Scene {
    pub fn new() -> Self {
        Self::with_integration_store(Rc::new(RefCell::new(SemanticStore::new())))
    }
    /// Integration entry point for language wrappers sharing one semantic arena.
    ///
    /// Creates a new root in the existing arena. It does not attach an existing
    /// session or publish changes into one; see [`crate::integration`].
    pub fn with_integration_store(store: Rc<RefCell<SemanticStore>>) -> Self {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_node(SemanticNodeCreation::family());
        let result = transaction
            .apply(&mut store.borrow_mut())
            .expect("empty family creation is valid");
        let [SemanticMutationImpact::NodeAdded { node: root }] = result.impacts() else {
            unreachable!("one family creation")
        };
        Self {
            store,
            root: *root,
            cursor: 0.0,
            execution: None,
        }
    }
    /// Raw shared arena access for explicit integration, not live mutation.
    ///
    /// External edits can invalidate generational handles and leave an existing
    /// execution session on a stale scene revision. Use Scene-owned persistent
    /// operations after lowering. No revision validation is bypassed by this
    /// accessor; see [`crate::integration`].
    pub fn integration_store(&self) -> &Rc<RefCell<SemanticStore>> {
        &self.store
    }
    /// Current authored revision without exposing mutable arena access.
    pub fn revision(&self) -> noon_core::SceneRevision {
        self.store.borrow().scene_revision()
    }

    /// Construct one detached Manim geometry object through the Scene-owned
    /// semantic publication path. The operation is valid before and after
    /// execution bootstrap; membership remains an explicit later operation.
    pub fn geometry(
        &mut self,
        options: crate::ManimGeometryOptions,
    ) -> Result<Mobject, crate::AuthoringError> {
        let root = self.root;
        let store_rc = Rc::clone(&self.store);
        let node = match self.execution.as_mut() {
            Some(execution) => {
                {
                    let store = store_rc.borrow();
                    execution
                        .require_resource_creation_at_root(&store, root)
                        .map_err(AuthoringError::from)?;
                }
                let mut store = store_rc.borrow_mut();
                publish_geometry_options(options, &mut store, |store, transaction| {
                    execution
                        .apply_semantic_transaction_at_root(store, root, transaction)
                        .map_err(AuthoringError::from)
                })?
            }
            None => {
                let mut store = store_rc.borrow_mut();
                publish_geometry_options(options, &mut store, |store, transaction| {
                    transaction.apply(store).map_err(AuthoringError::from)
                })?
            }
        };
        Mobject::from_node(store_rc, node)
    }

    /// Construct one detached Arrow-family object through the shared Rust
    /// transaction. The options select Arrow, Vector, or DoubleArrow semantics.
    pub fn manim_arrow(
        &self,
        options: crate::ManimArrowOptions,
    ) -> Result<crate::ManimArrow, crate::AuthoringError> {
        crate::ManimArrow::create(Rc::clone(&self.store), options)
    }

    pub fn root(&self) -> SemanticNodeId {
        self.root
    }
    pub fn time(&self) -> f64 {
        self.cursor
    }
    pub fn wait(&mut self, duration: f64) -> Result<(), String> {
        if !duration.is_finite() || duration < 0.0 || !(self.cursor + duration).is_finite() {
            return Err("duration must be finite and non-negative".into());
        }
        self.cursor += duration;
        Ok(())
    }
    pub fn circle(&self, radius: f64) -> Result<Mobject, crate::AuthoringError> {
        Mobject::manim_circle(Rc::clone(&self.store), radius)
    }
    pub fn square(&self, side: f64) -> Result<Mobject, crate::AuthoringError> {
        Mobject::manim_square(Rc::clone(&self.store), side)
    }
    pub fn rectangle(&self, width: f64, height: f64) -> Result<Mobject, crate::AuthoringError> {
        Mobject::manim_rectangle(Rc::clone(&self.store), width, height)
    }
    pub fn line(
        &self,
        start: (f64, f64),
        end: (f64, f64),
    ) -> Result<Mobject, crate::AuthoringError> {
        Mobject::manim_line(Rc::clone(&self.store), start.0, start.1, end.0, end.1)
    }
    pub fn path(
        &self,
        path: VectorPath,
        style: SemanticStyle,
    ) -> Result<Mobject, crate::AuthoringError> {
        Mobject::from_geometry(Rc::clone(&self.store), GeometryRef::path(path), style)
    }
    pub fn add(&mut self, object: &Mobject) -> Result<(), AuthoringError> {
        self.edit_membership(SceneMembershipRequest::Add(&[MobjectTarget::Object(
            object,
        )]))
        .map(|_| ())
    }

    /// Prepare and apply one atomic authored membership edit.
    ///
    /// Before execution bootstrap this commits only to the Semantic Scene. Once
    /// this Scene owns execution, the same transaction is published atomically
    /// through that execution component. No external Scene/session pairing is
    /// required for persistent membership edits.
    pub fn edit_membership(
        &mut self,
        request: SceneMembershipRequest<'_>,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        let transaction =
            crate::scene_membership::prepare_scene_membership(&self.store, self.root, request)?;
        self.apply_semantic_transaction(transaction)
    }

    /// Publish one canonical persistent transaction through a coherent running
    /// Scene root. Compatibility live facades reuse this route instead of owning
    /// another store/session publication branch.
    pub(crate) fn publish_running_transaction(
        store: &Rc<RefCell<SemanticStore>>,
        root: SemanticNodeId,
        execution: &mut ExecutionSession,
        transaction: SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, crate::ExecutionSessionPublicationError> {
        let mut store = store.borrow_mut();
        execution.apply_semantic_transaction_at_root(&mut store, root, transaction)
    }

    /// Route one canonical persistent semantic transaction through the Scene's
    /// current control state. This is the ownership seam for ordinary mutation:
    /// cold Scenes commit authored state directly; running Scenes use the existing
    /// prepared execution publication protocol without relowering or mirroring.
    pub(crate) fn apply_semantic_transaction(
        &mut self,
        transaction: SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        let root = self.root;
        let store = &self.store;
        if let Some(execution) = self.execution.as_mut() {
            return Self::publish_running_transaction(store, root, execution, transaction)
                .map_err(AuthoringError::from);
        }
        transaction
            .apply(&mut self.store.borrow_mut())
            .map_err(AuthoringError::from)
    }

    fn with_running_execution<T>(
        &mut self,
        operation: impl FnOnce(
            &Rc<RefCell<SemanticStore>>,
            SemanticNodeId,
            &mut ExecutionSession,
        ) -> Result<T, AuthoringError>,
    ) -> Result<T, AuthoringError> {
        let execution = self.execution.as_mut().ok_or(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::EffectiveStateUnavailable,
        ))?;
        operation(&self.store, self.root, execution)
    }

    pub fn add_many(
        &mut self,
        members: &[MobjectTarget<'_>],
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.edit_membership(SceneMembershipRequest::Add(members))
    }

    pub fn remove_many(
        &mut self,
        members: &[MobjectTarget<'_>],
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.edit_membership(SceneMembershipRequest::Remove(members))
    }

    pub fn clear(&mut self) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.edit_membership(SceneMembershipRequest::Clear)
    }

    pub fn replace(
        &mut self,
        old: MobjectTarget<'_>,
        new: MobjectTarget<'_>,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.edit_membership(SceneMembershipRequest::Replace { old, new })
    }

    /// Create one detached semantic family with authoritative ordered members.
    ///
    /// Construction uses this Scene's ordinary durable mutation route. Before
    /// execution bootstrap it commits authored state directly; while running it
    /// prepares and publishes the detached family atomically with the current
    /// execution revision. Membership remains an explicit later operation.
    pub fn family(
        &mut self,
        members: &[MobjectTarget<'_>],
    ) -> Result<MobjectFamily, crate::AuthoringError> {
        self.family_with_z_index(members, 0.0)
    }

    /// Construct a detached family with priority on its root only through this
    /// Scene's ordinary durable mutation route.
    pub fn family_with_z_index(
        &mut self,
        members: &[MobjectTarget<'_>],
        z_index: f64,
    ) -> Result<MobjectFamily, crate::AuthoringError> {
        let (transaction, family) =
            crate::family_authoring::family_creation_transaction(&self.store, members, z_index)?;
        let result = self.apply_semantic_transaction(transaction)?;
        let node = result
            .resolve(family)
            .expect("committed family token resolves to one semantic identity");
        MobjectFamily::from_node(Rc::clone(&self.store), node)
    }
    /// Create and scope a scalar input signal through this Scene's durable
    /// authoring route.
    ///
    /// Before bootstrap the signal is committed to authored state. While
    /// running, creation and reactive enrollment publish atomically through
    /// this Scene's owned execution component.
    pub fn value_tracker(&mut self, initial: f64) -> Result<ValueTracker, AuthoringError> {
        if let Some(execution) = self.execution.as_mut() {
            return crate::integration::publish_value_tracker_creation(
                &self.store,
                self.root,
                execution,
                initial,
            );
        }
        let creation = SemanticNodeCreation::input_signal(initial).map_err(AuthoringError::from)?;
        let mut transaction = SemanticMutationTransaction::new();
        let pending = transaction.create_node(creation);
        transaction.scope_signal(self.root, pending);
        let result = transaction
            .apply(&mut self.store.borrow_mut())
            .map_err(AuthoringError::from)?;
        let node = result
            .resolve(pending)
            .expect("committed tracker creation resolves its transaction-local token");
        Ok(ValueTracker::from_semantic_node(
            Rc::clone(&self.store),
            node,
        ))
    }

    /// Associate a detached scalar input through this Scene's durable
    /// authoring route.
    pub fn associate_value_tracker(
        &mut self,
        tracker: &ValueTracker,
    ) -> Result<(), AuthoringError> {
        tracker.require_store(&self.store)?;
        if let Some(execution) = self.execution.as_mut() {
            return crate::integration::publish_value_tracker_association(
                &self.store,
                self.root,
                execution,
                tracker,
            );
        }
        let mut transaction = SemanticMutationTransaction::new();
        transaction.scope_signal(self.root, tracker.node_id());
        transaction
            .apply(&mut self.store.borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub fn remove(&mut self, object: &Mobject) -> Result<(), AuthoringError> {
        self.edit_membership(SceneMembershipRequest::Remove(&[MobjectTarget::Object(
            object,
        )]))
        .map(|_| ())
    }
    /// Capture exact current path controls and transform from one coherent
    /// Runtime publication. Cold Scenes fail explicitly rather than falling back
    /// to authored geometry.
    pub fn effective_path_query(
        &self,
        object: &Mobject,
    ) -> Result<crate::PathQuery, AuthoringError> {
        let execution = self.execution.as_ref().ok_or(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::EffectiveStateUnavailable,
        ))?;
        crate::path_queries::effective_path_query(&self.store, execution, object)
    }

    /// Read one persistent authored/base object declaration.
    ///
    /// This deliberately does not inspect the running Runtime. Use
    /// [`Self::effective`] when the current published frame value is required.
    pub fn authored(
        &self,
        object: &Mobject,
    ) -> Result<noon_core::SemanticObjectState, AuthoringError> {
        self.require_object(object)?;
        object.state()
    }

    /// Read one object's current effective Runtime value from this Scene's
    /// coherent publication.
    ///
    /// Cold Scenes fail explicitly instead of substituting authored/base state.
    pub fn effective(
        &self,
        object: &Mobject,
    ) -> Result<crate::EffectiveMobjectState, AuthoringError> {
        self.require_object(object)?;
        let execution = self.execution.as_ref().ok_or(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::EffectiveStateUnavailable,
        ))?;
        let store = self.store.borrow();
        let observed = execution
            .effective_semantic_object(&store, object.node_id())
            .map_err(AuthoringError::from)?;
        Ok(crate::EffectiveMobjectState {
            z_index: observed.object.z_index,
            transform: observed.object.transform,
            style: observed.object.style,
            appearance: observed.object.appearance,
            publication: observed.publication,
        })
    }

    /// Match one object's persistent geometry/transform against another object.
    /// Cold Scenes use authored target state. Running Scenes capture the target
    /// from one coherent Runtime publication and publish through the Scene-owned
    /// execution component; unsupported derived presentation fails before commit.
    pub fn match_points(
        &mut self,
        source: &Mobject,
        target: &Mobject,
    ) -> Result<(), AuthoringError> {
        let root = self.root;
        let store = &self.store;
        if let Some(execution) = self.execution.as_mut() {
            return crate::point_matching::publish_match_points(
                store, root, execution, source, target,
            );
        }
        self.require_object(source)?;
        self.require_object(target)?;
        let transaction = crate::point_matching::prepare_match_points(source, target.state()?)?;
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    /// Align two persistent paths using authored state while cold and one coherent
    /// effective Runtime publication while running. Resource admission and publication
    /// stay owned by this Scene; no borrowed LiveSession facade is created.
    pub fn align_points(&mut self, left: &Mobject, right: &Mobject) -> Result<(), AuthoringError> {
        let root = self.root;
        let store = &self.store;
        if let Some(execution) = self.execution.as_mut() {
            return crate::path_alignment::publish_alignment(store, root, execution, left, right);
        }
        self.require_object(left)?;
        self.require_object(right)?;
        left.align_points(right)
    }

    /// Observe one family's effective Runtime layout without creating a borrowed
    /// LiveSession control facade. Cold Scenes fail explicitly rather than returning
    /// authored bounds as if they were an effective publication.
    pub fn effective_family_layout(
        &self,
        family: &MobjectFamily,
    ) -> Result<crate::EffectiveMobjectLayout, AuthoringError> {
        let execution = self.execution.as_ref().ok_or(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::EffectiveStateUnavailable,
        ))?;
        crate::family_layout::effective_family_layout(&self.store, execution, family)
    }

    /// Shift a family through the Scene-owned running publication authority.
    /// Use `MobjectFamily::shift` for cold authored placement.
    pub fn shift_family(
        &mut self,
        family: &MobjectFamily,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            crate::family_layout::publish_shift_family(store, root, execution, family, x, y)
        })
    }

    /// Arrange a family from current effective bounds and publish once.
    pub fn arrange_family_with_options(
        &mut self,
        family: &MobjectFamily,
        options: &crate::FamilyArrangeOptions,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            crate::family_layout::publish_arrange_family(store, root, execution, family, options)
        })
    }

    /// Arrange a family grid from current effective bounds and publish once.
    pub fn arrange_family_in_grid_with_options(
        &mut self,
        family: &MobjectFamily,
        options: &crate::FamilyGridOptions,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            crate::family_layout::publish_arrange_family_in_grid(
                store, root, execution, family, options,
            )
        })
    }

    /// Move one object relative to an effective target through one transaction.
    pub fn move_to(
        &mut self,
        object: &Mobject,
        target: crate::LiveLayoutTarget<'_>,
        edge: (f64, f64),
        mask: (f64, f64),
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            crate::family_layout::publish_move_to(
                store, root, execution, object, target, edge, mask,
            )
        })
    }

    /// Move a family relative to an effective target through one transaction.
    pub fn move_family_to(
        &mut self,
        family: &MobjectFamily,
        target: crate::LiveLayoutTarget<'_>,
        edge: (f64, f64),
        mask: (f64, f64),
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            crate::family_layout::publish_move_family_to(
                store, root, execution, family, target, edge, mask,
            )
        })
    }

    /// Place a family next to one effective target through one transaction.
    pub fn next_family_to(
        &mut self,
        family: &MobjectFamily,
        target: crate::LiveLayoutTarget<'_>,
        args: crate::ManimNextToArgs,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            crate::family_layout::publish_next_family_to(
                store, root, execution, family, target, args,
            )
        })
    }

    /// Align a family to the default frame using current effective bounds.
    pub fn align_family_on_frame(
        &mut self,
        family: &MobjectFamily,
        direction: (f64, f64),
        buff: f64,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            crate::family_layout::publish_align_family_on_frame(
                store, root, execution, family, direction, buff,
            )
        })
    }

    /// Align a family to one effective target using current effective bounds.
    pub fn align_family_to(
        &mut self,
        family: &MobjectFamily,
        target: crate::LiveLayoutTarget<'_>,
        axis: (f64, f64),
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            crate::family_layout::publish_align_family_to(
                store, root, execution, family, target, axis,
            )
        })
    }

    /// Place one selected layout using a distinct selected aligner.
    pub fn next_layout_to_aligned(
        &mut self,
        source: &crate::LayoutAnchor,
        target: crate::LiveLayoutTarget<'_>,
        aligner: &crate::LayoutAnchor,
        args: crate::ManimNextToArgs,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            crate::family_layout::publish_next_layout_to_aligned(
                store, root, execution, source, target, aligner, args,
            )
        })
    }

    /// Observe effective Runtime priority without creating a borrowed LiveSession facade.
    /// Cold Scenes fail explicitly rather than returning authored state as effective state.
    pub fn effective_z_index(&self, source: &crate::LayoutAnchor) -> Result<f64, AuthoringError> {
        let execution = self.execution.as_ref().ok_or(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::EffectiveStateUnavailable,
        ))?;
        crate::z_index::effective_z_index(&self.store, execution, source)
    }

    /// Prepare boolean geometry from the current coherent Runtime publication.
    /// Cold Scenes fail explicitly rather than substituting authored geometry.
    pub fn effective_boolean_geometry_options(
        &self,
        operation: crate::BooleanOperation,
        operands: &[Mobject],
    ) -> Result<crate::ManimGeometryOptions, AuthoringError> {
        let execution = self.execution.as_ref().ok_or(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::EffectiveStateUnavailable,
        ))?;
        crate::boolean_authoring::effective_boolean_geometry_options(
            &self.store,
            execution,
            operation,
            operands,
        )
    }

    pub(crate) fn require_object(&self, object: &Mobject) -> Result<(), crate::AuthoringError> {
        if !Rc::ptr_eq(&self.store, object.integration_store()) {
            return Err(crate::AuthoringError::ForeignStore);
        }
        object.validate()
    }
    /// Validate the bounded ordinary leaf-affine operation without creating a
    /// declaration, session, track, or runtime identity.
    pub fn can_ordinary_transform_to(
        &self,
        source: &Mobject,
        target: &Mobject,
        options: AnimationOptions,
    ) -> Result<bool, String> {
        self.require_object(source)
            .map_err(|error| error.to_string())?;
        self.require_object(target)
            .map_err(|error| error.to_string())?;
        match noon_compile::validate_semantic_transform_to_payload(
            &self.store.borrow(),
            source.node_id(),
            target.node_id(),
            options,
        ) {
            Ok(()) => {}
            Err(error) if error.is_unsupported_payload() => return Ok(false),
            Err(error) => return Err(error.to_string()),
        }
        // The ordinary path uses the shared track timing for both linear and
        // Manim's default smooth curve. Other endpoint policies remain explicit.
        if !matches!(
            options.rate_func,
            None | Some(RateFunction::Linear | RateFunction::Smooth)
        ) {
            return Ok(false);
        }
        Ok(true)
    }
    pub fn execution_session(
        &self,
    ) -> Result<ExecutionSession, noon_compile::SemanticExecutionLoweringError> {
        ExecutionSession::from_semantic_root(&self.store.borrow(), self.root)
    }

    /// Lower this scene and one explicit authored animation root into the shared runtime.
    ///
    /// The root must belong to this scene's store. Exact property tracks keep their
    /// authored timing; neutral family compositions start at zero. Declarations
    /// outside this root are not scheduled.
    pub fn execution_session_with_animation_root(
        &self,
        animation_root: &crate::DeclaredAnimation,
    ) -> Result<ExecutionSession, String> {
        self.execution_session_with_animation_root_at(animation_root, 0.0)
    }

    /// Start initial family compositions at an explicit origin, including before time zero.
    /// Exact property tracks retain their own absolute timing.
    pub fn execution_session_with_animation_root_at(
        &self,
        animation_root: &crate::DeclaredAnimation,
        origin: f64,
    ) -> Result<ExecutionSession, String> {
        animation_root.require_store(&self.store)?;
        ExecutionSession::from_semantic_root_with_animation_root_at(
            &self.store.borrow(),
            self.root,
            animation_root.node_id(),
            origin,
        )
        .map_err(|error| error.to_string())
    }

    /// Install the execution component lowered from this exact Scene.
    ///
    /// This is private migration plumbing for the Scene-owned live path. External
    /// integration entry points may still construct standalone sessions until the
    /// broader control-surface migration removes that compatibility shape.
    pub(crate) fn install_execution(&mut self, execution: ExecutionSession) {
        assert!(
            self.execution.is_none(),
            "scene execution component may only be installed once"
        );
        self.execution = Some(execution);
    }

    pub(crate) const fn owned_execution(&self) -> &ExecutionSession {
        match &self.execution {
            Some(execution) => execution,
            None => panic!("scene execution component is not initialized"),
        }
    }

    pub(crate) fn owned_execution_mut(&mut self) -> &mut ExecutionSession {
        self.execution
            .as_mut()
            .expect("scene execution component is not initialized")
    }

    pub(crate) fn owned_live(&mut self) -> LiveSession<'_> {
        let root = self.root;
        let store = &self.store;
        let execution = self
            .execution
            .as_mut()
            .expect("scene execution component is not initialized");
        LiveSession::new(store, root, execution)
    }

    /// Borrow an explicitly supplied execution session for compatibility during
    /// the control-surface migration. Scene-owned persistent operations do not
    /// require this pairing once execution is installed in the Scene itself.
    pub fn live<'a>(&'a self, session: &'a mut ExecutionSession) -> LiveSession<'a> {
        LiveSession::new(&self.store, self.root, session)
    }
}
#[cfg(test)]
mod geometry_atomicity_tests;
#[cfg(test)]
mod tests;
