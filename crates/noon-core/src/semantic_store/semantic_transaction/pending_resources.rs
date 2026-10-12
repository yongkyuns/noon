//! Transaction-owned immutable resource payloads for provisional construction.
//!
//! Raw paths stay outside all store arenas until a prepared transaction reaches
//! its final synchronous publication scope. This child module can operate on the
//! transaction's private fields without exposing a second resource registry.

use super::*;

// Ordinary property transactions carry no resource allocation or vector header.
// Keep the monotonic token counter with the lazily allocated resource batch.
#[derive(Debug, Default, PartialEq)]
pub(super) struct PendingResourceDeclarations {
    next_token: u32,
    paths: Vec<(SemanticLocalResourceToken, VectorPath)>,
}

impl SemanticMutationTransaction {
    pub(super) fn pending_resource_count(&self) -> usize {
        self.pending_resources
            .as_ref()
            .map_or(0, |resources| resources.paths.len())
    }

    pub(super) fn truncate_pending_resources(&mut self, len: usize) {
        if let Some(resources) = self.pending_resources.as_mut() {
            resources.paths.truncate(len);
        }
    }

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
        // Count the complete payload before recursive finite validation. Nested
        // morph targets cannot bypass the working-set or nesting-depth bound.
        let commands = Self::pending_path_command_count(&path)
            .ok_or(SemanticMutationTransactionError::PendingGeometryLimitExceeded)?;
        if !path.is_finite() {
            return Err(SemanticMutationTransactionError::InvalidPendingGeometryPath);
        }
        if self.pending_resource_count() == Self::MAX_PENDING_GEOMETRY_RESOURCES
            || self
                .pending_resources
                .iter()
                .flat_map(|resources| resources.paths.iter())
                .map(|(_, path)| {
                    Self::pending_path_command_count(path)
                        .expect("admitted pending path is bounded")
                })
                .sum::<usize>()
                .saturating_add(commands)
                > Self::MAX_PENDING_GEOMETRY_COMMANDS
        {
            return Err(SemanticMutationTransactionError::PendingGeometryLimitExceeded);
        }
        let resources = self.pending_resources.get_or_insert_with(Default::default);
        let ordinal = resources.next_token;
        resources.next_token = resources
            .next_token
            .checked_add(1)
            .ok_or(SemanticMutationTransactionError::LocalResourceTokenExhausted)?;
        let token = SemanticLocalResourceToken::new(self.id, ordinal);
        resources.paths.push((token, path));
        Ok(token)
    }

    fn pending_path_command_count(path: &VectorPath) -> Option<usize> {
        let mut current = Some(path);
        let mut commands = 0usize;
        for _ in 0..Self::MAX_PENDING_GEOMETRY_RESOURCES {
            let Some(path) = current else {
                return Some(commands);
            };
            commands = commands.checked_add(path.commands().len())?;
            if commands > Self::MAX_PENDING_GEOMETRY_COMMANDS {
                return None;
            }
            current = path.morph_target();
        }
        current.is_none().then_some(commands)
    }

    pub(super) fn pending_geometry_path(
        &self,
        token: SemanticLocalResourceToken,
    ) -> Option<&VectorPath> {
        if !token.belongs_to(self.id) {
            return None;
        }
        self.pending_resources
            .as_ref()?
            .paths
            .iter()
            .find_map(|(candidate, path)| (*candidate == token).then_some(path))
    }

    pub(super) fn take_pending_geometry_paths(
        &mut self,
        tokens: &[SemanticLocalResourceToken],
    ) -> Vec<(SemanticLocalResourceToken, VectorPath)> {
        // Every unselected payload belongs to a canceled pending node and is
        // discarded here rather than admitted into the resource arena.
        self.pending_resources
            .as_mut()
            .map(|resources| std::mem::take(&mut resources.paths))
            .unwrap_or_default()
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
