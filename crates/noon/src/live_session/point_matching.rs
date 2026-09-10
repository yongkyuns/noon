use super::*;
use crate::point_matching::matched_state;

impl LiveSession<'_> {
    /// Match geometry from one coherent effective target observation. Derived
    /// render overrides use the existing capture rejection instead of stale data.
    /// Like other persistent edits, this requires a completed continuation boundary.
    pub fn match_points(
        &mut self,
        source: &Mobject,
        target: &Mobject,
    ) -> Result<(), LiveSessionError> {
        self.require_mobject(source)?;
        self.require_mobject(target)?;
        self.capture_mobject_state(source)?;
        let before = source.state()?;
        let after = matched_state(before.clone(), self.capture_mobject_state(target)?)?;
        let mut transaction = SemanticMutationTransaction::new();
        crate::semantic_mobject::stage_state_changes(
            &mut transaction,
            source.node_id(),
            &before,
            &after,
        );
        self.apply(transaction).map(|_| ())
    }
}
