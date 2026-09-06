//! Direct authoring scope over one family in the shared semantic store.
use crate::{ExecutionSession, LiveSession, Mobject};
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
        Self::with_store(Rc::new(RefCell::new(SemanticStore::new())))
    }
    /// Integration entry point for language wrappers sharing one semantic arena.
    pub fn with_store(store: Rc<RefCell<SemanticStore>>) -> Self {
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
    /// Integration access; handles reject stale identities after external edits.
    pub fn store(&self) -> &Rc<RefCell<SemanticStore>> {
        &self.store
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
    pub fn circle(&self, radius: f64) -> Result<Mobject, String> {
        Mobject::manim_circle(Rc::clone(&self.store), radius)
    }
    pub fn square(&self, side: f64) -> Result<Mobject, String> {
        Mobject::manim_square(Rc::clone(&self.store), side)
    }
    pub fn rectangle(&self, width: f64, height: f64) -> Result<Mobject, String> {
        Mobject::manim_rectangle(Rc::clone(&self.store), width, height)
    }
    pub fn line(&self, start: (f64, f64), end: (f64, f64)) -> Result<Mobject, String> {
        Mobject::manim_line(Rc::clone(&self.store), start.0, start.1, end.0, end.1)
    }
    pub fn path(&self, path: VectorPath, style: SemanticStyle) -> Result<Mobject, String> {
        Mobject::from_geometry(Rc::clone(&self.store), GeometryRef::path(path), style)
    }
    pub fn add(&mut self, object: &Mobject) -> Result<(), String> {
        self.require_object(object)?;
        self.add_node(object.node_id())
    }
    pub fn remove(&mut self, object: &Mobject) -> Result<(), String> {
        self.require_object(object)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.remove_member(self.root, object.node_id());
        transaction
            .apply(&mut self.store.borrow_mut())
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    /// Integration entry point for retained feature nodes in the same store.
    /// Callers must first establish that `node` originated in `store()`.
    pub(crate) fn add_node(&mut self, node: SemanticNodeId) -> Result<(), String> {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_member(self.root, node);
        transaction
            .apply(&mut self.store.borrow_mut())
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    pub(crate) fn require_object(&self, object: &Mobject) -> Result<(), String> {
        if !Rc::ptr_eq(&self.store, object.store()) {
            return Err("mobject belongs to another scene store".into());
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
        self.require_object(source)?;
        self.require_object(target)?;
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

    /// Borrow the already-published execution session for supported live membership,
    /// property edits, and effective-value queries. This facade retains no scene/runtime state.
    pub fn live<'a>(&'a self, session: &'a mut ExecutionSession) -> LiveSession<'a> {
        LiveSession::new(&self.store, self.root, session)
    }
}
#[cfg(test)]
mod tests;
