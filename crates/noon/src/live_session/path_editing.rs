use super::*;

impl LiveSession<'_> {
    pub(crate) fn publish_path_edits(
        &mut self,
        prepared: crate::path_editing::PreparedPathEdits,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        crate::path_editing::publish_running_path_edits(
            self.store,
            self.root,
            self.session,
            prepared,
        )
        .map_err(LiveSessionError::from)
    }
}

impl From<noon_core::GeometryResourceError> for LiveSessionError {
    fn from(error: noon_core::GeometryResourceError) -> Self {
        crate::AuthoringError::from(error).into()
    }
}
