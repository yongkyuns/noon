//! Coordinate adapters for the player that owns the running Scene.
use super::*;

impl SemanticExecutionPlayer {
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_create_path_family(
        &mut self,
        paths: Vec<(noon::VectorPath, noon_core::SemanticStyle)>,
    ) -> Result<noon::MobjectFamily, AuthoringFailure> {
        self.with_live_session(|live| Ok(live.create_path_family(paths)))?
            .map_err(AuthoringFailure::from)
    }
}
