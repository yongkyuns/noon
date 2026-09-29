//! Transaction-owned immutable resource payloads for provisional construction.
//!
//! Raw paths stay outside all store arenas until a prepared transaction reaches
//! its final synchronous publication scope. This child module can operate on the
//! transaction's private fields without exposing a second resource registry.

use super::*;

impl SemanticMutationTransaction {
    /// Maximum transaction-local immutable path declarations. Callback collectors
    /// impose the same bounded working-set rule before final publication.
    pub const MAX_PENDING_GEOMETRY_RESOURCES: usize = 128;
    /// Maximum command count retained by one transaction-local path payload batch.
    pub const MAX_PENDING_GEOMETRY_COMMANDS: usize = 65_536;

    /// Retain one finite immutable vector path in this transaction only.
    ///
    /// The returned token is not a geometry handle and cannot be rendered or
    /// queried through a store. It can be consumed only by the transaction-only
    /// `SemanticNodeCreation::PendingPathObject` declaration; the prepared
    /// resource publication scope resolves that declaration atomically at commit.
    pub fn stage_geometry_path(
        &mut self,
        path: VectorPath,
    ) -> Result<SemanticLocalResourceToken, SemanticMutationTransactionError> {
        if !path.is_finite() {
            return Err(SemanticMutationTransactionError::InvalidPendingGeometryPath);
        }
        if self.pending_geometry_paths.len() == Self::MAX_PENDING_GEOMETRY_RESOURCES
            || path.commands().len() > Self::MAX_PENDING_GEOMETRY_COMMANDS
            || self
                .pending_geometry_paths
                .iter()
                .map(|(_, path)| path.commands().len())
                .sum::<usize>()
                .saturating_add(path.commands().len())
                > Self::MAX_PENDING_GEOMETRY_COMMANDS
        {
            return Err(SemanticMutationTransactionError::PendingGeometryLimitExceeded);
        }
        let ordinal = self.next_resource_token;
        self.next_resource_token = self
            .next_resource_token
            .checked_add(1)
            .ok_or(SemanticMutationTransactionError::LocalResourceTokenExhausted)?;
        let token = SemanticLocalResourceToken::new(self.id, ordinal);
        self.pending_geometry_paths.push((token, path));
        Ok(token)
    }

    pub(super) fn pending_geometry_path(
        &self,
        token: SemanticLocalResourceToken,
    ) -> Option<&VectorPath> {
        token
            .belongs_to(self.id)
            .then(|| {
                self.pending_geometry_paths
                    .iter()
                    .find_map(|(candidate, path)| (*candidate == token).then_some(path))
            })
            .flatten()
    }

    pub(super) fn take_pending_geometry_paths(
        &mut self,
        tokens: &[SemanticLocalResourceToken],
    ) -> Vec<(SemanticLocalResourceToken, VectorPath)> {
        // Every unselected payload belongs to a canceled pending node and is
        // discarded here rather than admitted into the resource arena.
        std::mem::take(&mut self.pending_geometry_paths)
            .into_iter()
            .filter(|(token, _)| tokens.contains(token))
            .collect()
    }

    pub(super) fn materialize_pending_geometry_paths(
        &mut self,
        handles: &std::collections::HashMap<
            SemanticLocalResourceToken,
            crate::GeometryResourceHandle,
        >,
        final_states: &HashMap<SemanticLocalNodeToken, SemanticPendingPathObject>,
    ) {
        for mutation in &mut self.mutations {
            let SemanticMutation::AddNode { token, creation } = mutation else {
                continue;
            };
            let SemanticNodeCreation::PendingPathObject {
                state,
                source_identity,
            } = creation
            else {
                continue;
            };
            // Canceled local nodes remain in the mutation log so their token
            // cannot be reused, but their path payload never enters an arena.
            let Some(final_state) = final_states.get(token) else {
                continue;
            };
            let resource = state.resource();
            let Some(handle) = handles.get(&resource).copied() else {
                // This map is built from the exact deduplicated resource tokens
                // named by `final_states`; an absent entry cannot arise from a
                // successfully prepared public transaction.
                continue;
            };
            *creation = SemanticNodeCreation::Object {
                state: Box::new(final_state.clone().materialize(handle)),
                source_identity: source_identity.clone(),
            };
        }
    }
}
