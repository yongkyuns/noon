//! Code construction uses the same atomic live publication as other text.
use super::SemanticExecutionPlayer;
use crate::authoring_error::AuthoringFailure;

impl SemanticExecutionPlayer {
    pub(crate) fn live_create_code(
        &mut self,
        source: &str,
        options: noon::CodeOptions,
    ) -> Result<noon::Code, AuthoringFailure> {
        noon::Code::new(self.source.scene_mut(), source, options).map_err(AuthoringFailure::from)
    }
}
