//! Direct authoring scope over one family in the shared semantic store.
use crate::{
    AuthoringError, ExecutionSession, LiveSession, Mobject, MobjectFamily, MobjectFamilyMember,
    SceneMembershipRequest,
};
use noon_core::{
    AnimationOptions, GeometryRef, RateFunction, SemanticMutationImpact,
    SemanticMutationTransaction, SemanticNodeCreation, SemanticNodeId, SemanticStore,
    SemanticStyle, VectorPath,
};
use std::{cell::RefCell, rc::Rc};

/// A scene owns only its shared semantic store, root identity, and authoring cursor.
/// Direct membership edits prepare a subsequent execution session. Use
/// [`LiveSession`] to publish supported membership changes into an existing session.
#[derive(Debug)]
pub struct Scene {
    store: Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    cursor: f64,
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
        }
    }
    /// Raw shared arena access for explicit integration, not live mutation.
    ///
    /// External edits can invalidate generational handles and leave an existing
    /// execution session on a stale scene revision. Use `Scene::live` and its
    /// coherent publication operations for edits after lowering. No revision
    /// validation is bypassed by this accessor; see [`crate::integration`].
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
        self.edit_membership(SceneMembershipRequest::Add(&[
            MobjectFamilyMember::Mobject(object),
        ]))
        .map(|_| ())
    }

    /// Prepare and apply one atomic authored membership edit.
    ///
    /// Errors preserve semantic identities and causes in [`AuthoringError`].
    /// After execution bootstrap, use [`LiveSession::edit_membership`] instead:
    /// direct authored edits invalidate that session's publication revision.
    pub fn edit_membership(
        &mut self,
        request: SceneMembershipRequest<'_>,
    ) -> Result<noon_core::SemanticMutationTransactionResult, AuthoringError> {
        let transaction =
            crate::scene_membership::prepare_scene_membership(&self.store, self.root, request)?;
        crate::scene_membership::apply_scene_membership(&self.store, transaction)
    }

    pub fn add_many(
        &mut self,
        members: &[MobjectFamilyMember<'_>],
    ) -> Result<noon_core::SemanticMutationTransactionResult, AuthoringError> {
        self.edit_membership(SceneMembershipRequest::Add(members))
    }

    pub fn remove_many(
        &mut self,
        members: &[MobjectFamilyMember<'_>],
    ) -> Result<noon_core::SemanticMutationTransactionResult, AuthoringError> {
        self.edit_membership(SceneMembershipRequest::Remove(members))
    }

    pub fn clear(
        &mut self,
    ) -> Result<noon_core::SemanticMutationTransactionResult, AuthoringError> {
        self.edit_membership(SceneMembershipRequest::Clear)
    }

    pub fn replace(
        &mut self,
        old: MobjectFamilyMember<'_>,
        new: MobjectFamilyMember<'_>,
    ) -> Result<noon_core::SemanticMutationTransactionResult, AuthoringError> {
        self.edit_membership(SceneMembershipRequest::Replace { old, new })
    }

    /// Create one detached semantic family with authoritative ordered members.
    pub fn family(
        &self,
        members: &[MobjectFamilyMember<'_>],
    ) -> Result<MobjectFamily, crate::AuthoringError> {
        self.family_with_z_index(members, 0.0)
    }

    /// Construct a detached family with priority on its root only.
    pub fn family_with_z_index(
        &self,
        members: &[MobjectFamilyMember<'_>],
        z_index: f64,
    ) -> Result<MobjectFamily, crate::AuthoringError> {
        MobjectFamily::create_with_z_index(Rc::clone(&self.store), members, z_index)
    }
    pub fn remove(&mut self, object: &Mobject) -> Result<(), AuthoringError> {
        self.edit_membership(SceneMembershipRequest::Remove(&[
            MobjectFamilyMember::Mobject(object),
        ]))
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

    /// Borrow the already-published execution session for supported live membership,
    /// property edits, and effective-value queries. This facade retains no scene/runtime state.
    pub fn live<'a>(&'a self, session: &'a mut ExecutionSession) -> LiveSession<'a> {
        LiveSession::new(&self.store, self.root, session)
    }
}
#[cfg(test)]
mod tests;
