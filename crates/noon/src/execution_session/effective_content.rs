use noon_core::{ObjectContentRef, Rect, SemanticNodeId};
use noon_runtime::{
    EffectiveContentError, EffectiveContentLease, PreparedEffectiveContentReplacement,
};

use super::ExecutionSession;

#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionSessionContentError {
    RequiredCallbackPending,
    SegmentCompletionPending,
    UnknownObject(SemanticNodeId),
    Runtime(EffectiveContentError),
}

impl std::fmt::Display for ExecutionSessionContentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RequiredCallbackPending => {
                formatter.write_str("a required callback phase is pending")
            }
            Self::SegmentCompletionPending => {
                formatter.write_str("a segment completion publication is pending")
            }
            Self::UnknownObject(node) => write!(
                formatter,
                "semantic object {}:{} is not live",
                node.slot(),
                node.generation()
            ),
            Self::Runtime(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ExecutionSessionContentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Runtime(error) => Some(error),
            _ => None,
        }
    }
}

impl From<EffectiveContentError> for ExecutionSessionContentError {
    fn from(value: EffectiveContentError) -> Self {
        Self::Runtime(value)
    }
}

impl ExecutionSession {
    /// Return this session incarnation's current lease for an explicitly
    /// adopted clone or producer handoff.
    pub fn effective_content_lease(&self, target: SemanticNodeId) -> Option<EffectiveContentLease> {
        let object = self.execution_index.execution_object_id(target)?;
        self.runtime.effective_content_lease(object)
    }

    fn require_effective_content_ready(&self) -> Result<(), ExecutionSessionContentError> {
        if self.pending_callback.is_some() {
            return Err(ExecutionSessionContentError::RequiredCallbackPending);
        }
        if self.pending_segment_completion.is_some() {
            return Err(ExecutionSessionContentError::SegmentCompletionPending);
        }
        Ok(())
    }

    /// Stage a versioned effective content result against this session's exact
    /// runtime/frame publication. It does not mutate authored semantic state.
    pub fn prepare_effective_content_replacement(
        &self,
        target: SemanticNodeId,
        content: ObjectContentRef,
        text_bounds: Option<Rect>,
        lease: Option<EffectiveContentLease>,
    ) -> Result<PreparedEffectiveContentReplacement, ExecutionSessionContentError> {
        self.require_effective_content_ready()?;
        let object = self
            .execution_index
            .execution_object_id(target)
            .ok_or(ExecutionSessionContentError::UnknownObject(target))?;
        self.runtime
            .prepare_effective_content_replacement(object, content, text_bounds, lease)
            .map_err(Into::into)
    }

    /// Publish one prepared effective result and refit only its changed spatial
    /// slot/dependencies. The semantic revision and compiled plan stay pinned.
    pub fn commit_effective_content_replacement(
        &mut self,
        prepared: PreparedEffectiveContentReplacement,
    ) -> Result<EffectiveContentLease, ExecutionSessionContentError> {
        self.require_effective_content_ready()?;
        let lease = self
            .runtime
            .commit_effective_content_replacement(prepared)?;
        self.sync_spatial_index();
        Ok(lease)
    }

    pub fn release_effective_content(
        &mut self,
        lease: EffectiveContentLease,
    ) -> Result<(), ExecutionSessionContentError> {
        self.require_effective_content_ready()?;
        self.runtime.release_effective_content(lease)?;
        self.sync_spatial_index();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        GeometryRef, ObjectContentRef, SemanticObjectState, SemanticStore, StoredGeometry, Vec2,
    };

    use super::super::picking::PointerFillOutcome;
    use super::ExecutionSession;

    #[test]
    fn effective_content_publication_updates_shared_spatial_picking_and_release() {
        let mut store = SemanticStore::new();
        let target =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(target).unwrap();
        let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
        let point = Vec2::new(2.0, 0.0);
        assert_eq!(
            session.pick_effective_fill(point, |_| true).0,
            PointerFillOutcome::Miss
        );
        let prepared = session
            .prepare_effective_content_replacement(
                target,
                ObjectContentRef::Geometry(GeometryRef::circle(3.0)),
                None,
                None,
            )
            .unwrap();
        let lease = session
            .commit_effective_content_replacement(prepared)
            .unwrap();
        assert_eq!(
            session.pick_effective_fill(point, |_| true).0,
            PointerFillOutcome::Hit(target)
        );
        session.release_effective_content(lease).unwrap();
        assert_eq!(
            session.pick_effective_fill(point, |_| true).0,
            PointerFillOutcome::Miss
        );
    }
}
