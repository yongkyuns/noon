//! Point matching replaces geometry through one shared semantic transaction.
use crate::{AuthoringError, Mobject, UnsupportedAuthoringOperation};
use noon_core::{SemanticMutationTransaction, SemanticObjectContent, SemanticObjectState};

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
