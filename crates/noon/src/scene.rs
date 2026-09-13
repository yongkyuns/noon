//! Direct authoring scope over one family in the shared semantic store.
use crate::{
    AuthoringError, ExecutionSession, LiveSession, Mobject, MobjectFamily, MobjectTarget,
    SceneMembershipRequest,
};
use noon_core::{
    AnimationOptions, GeometryRef, RateFunction, SemanticMutationImpact,
    SemanticMutationTransaction, SemanticMutationTransactionResult, SemanticNodeCreation,
    SemanticNodeId, SemanticStore, SemanticStyle, VectorPath,
};
use std::{cell::RefCell, rc::Rc};

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

    /// Construct detached geometry through the shared authoring implementation.
    /// Use `LiveSession::create_manim_geometry` after initial lowering instead.
    pub fn geometry(
        &self,
        options: crate::ManimGeometryOptions,
    ) -> Result<Mobject, crate::AuthoringError> {
        Mobject::from_manim_geometry(Rc::clone(&self.store), options)
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

    /// Route one canonical persistent semantic transaction through the Scene's
    /// current control state. This is the ownership seam for ordinary mutation:
    /// cold Scenes commit authored state directly; running Scenes use the existing
    /// prepared execution publication protocol without relowering or mirroring.
    pub(crate) fn apply_semantic_transaction(
        &mut self,
        transaction: SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError> {
        if let Some(execution) = self.execution.as_mut() {
            let mut store = self.store.borrow_mut();
            return execution
                .apply_semantic_transaction_at_root(&mut store, self.root, transaction)
                .map_err(AuthoringError::from);
        }
        transaction
            .apply(&mut self.store.borrow_mut())
            .map_err(AuthoringError::from)
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
    pub fn family(
        &self,
        members: &[MobjectTarget<'_>],
    ) -> Result<MobjectFamily, crate::AuthoringError> {
        self.family_with_z_index(members, 0.0)
    }

    /// Construct a detached family with priority on its root only.
    pub fn family_with_z_index(
        &self,
        members: &[MobjectTarget<'_>],
        z_index: f64,
    ) -> Result<MobjectFamily, crate::AuthoringError> {
        MobjectFamily::create_with_z_index(Rc::clone(&self.store), members, z_index)
    }
    pub fn remove(&mut self, object: &Mobject) -> Result<(), AuthoringError> {
        self.edit_membership(SceneMembershipRequest::Remove(&[MobjectTarget::Object(
            object,
        )]))
        .map(|_| ())
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
mod tests;
