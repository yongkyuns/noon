//! Point matching replaces geometry through one shared semantic transaction.
use crate::{AuthoringError, ExecutionSession, Mobject, UnsupportedAuthoringOperation};
use noon_core::{
    SemanticMutationTransaction, SemanticNodeId, SemanticObjectContent, SemanticObjectState,
    SemanticStore,
};
use std::{cell::RefCell, rc::Rc};

pub(crate) fn matched_state(
    mut source: SemanticObjectState,
    target: SemanticObjectState,
) -> Result<SemanticObjectState, AuthoringError> {
    if !matches!(source.content, SemanticObjectContent::Geometry(_))
        || !matches!(target.content, SemanticObjectContent::Geometry(_))
    {
        return Err(AuthoringError::Unsupported(
            UnsupportedAuthoringOperation::PointMatchContent,
        ));
    }
    source.content = target.content;
    source.transform = target.transform;
    Ok(source)
}

pub(crate) fn prepare_match_points(
    source: &Mobject,
    target: SemanticObjectState,
) -> Result<SemanticMutationTransaction, AuthoringError> {
    let before = source.state()?;
    let after = matched_state(before.clone(), target)?;
    let mut transaction = SemanticMutationTransaction::new();
    crate::semantic_mobject::stage_state_changes(
        &mut transaction,
        source.node_id(),
        &before,
        &after,
    );
    Ok(transaction)
}

pub fn publish_match_points(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    source: &Mobject,
    target: &Mobject,
) -> Result<(), AuthoringError> {
    if !Rc::ptr_eq(store, source.integration_store())
        || !Rc::ptr_eq(store, target.integration_store())
    {
        return Err(AuthoringError::ForeignStore);
    }
    source.validate()?;
    target.validate()?;
    crate::effective_capture::capture_mobject_state(store, execution, source)?;
    let target_state = crate::effective_capture::capture_mobject_state(store, execution, target)?;
    let transaction = prepare_match_points(source, target_state)?;
    crate::Scene::publish_running_transaction(store, root, execution, transaction)
        .map(|_| ())
        .map_err(AuthoringError::from)
}

impl Mobject {
    /// Match another vector object's world points without copying its paint,
    /// priority, identity or bindings. Immutable geometry resources are shared.
    pub fn match_points(&mut self, target: &Self) -> Result<(), AuthoringError> {
        self.require_same_store(target)?;
        prepare_match_points(self, target.state()?)?
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }
}
