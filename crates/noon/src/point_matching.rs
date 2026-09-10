//! Point matching replaces geometry through the shared semantic transaction.
use crate::{AuthoringError, Mobject, UnsupportedAuthoringOperation};
use noon_core::{SemanticObjectContent, SemanticObjectState};

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

impl Mobject {
    /// Match another vector object's world points without copying its paint,
    /// priority, identity or bindings. Immutable geometry resources are shared.
    pub fn match_points(&mut self, target: &Self) -> Result<(), AuthoringError> {
        self.require_same_store(target)?;
        self.commit_state(matched_state(self.state()?, target.state()?)?)
    }
}
