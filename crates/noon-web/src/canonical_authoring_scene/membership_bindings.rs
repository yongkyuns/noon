use super::*;

impl CanonicalAuthoringScene {
    /// Validate derived wrapper identities without changing semantic membership.
    pub(super) fn prepare_membership_bindings(
        &self,
        batch: &SceneMembershipBatch,
    ) -> Result<Vec<(ObjectId, noon_core::SemanticNodeId)>, AuthoringFailure> {
        let mut new_bindings = Vec::new();
        let mut seen_ids = BTreeSet::new();
        let mut seen_nodes = BTreeSet::new();
        for (wrapper_id, handle) in &batch.bindings {
            if !std::rc::Rc::ptr_eq(self.scene.integration_store(), handle.integration_store()) {
                return Err(AuthoringFailure::from(noon::AuthoringError::ForeignStore)
                    .with_message("membership mobject belongs to another authoring store"));
            }
            handle.validate().map_err(AuthoringFailure::from)?;
            let node = handle.node_id();
            if !seen_ids.insert(*wrapper_id) || !seen_nodes.insert(node) {
                return Err(AuthoringFailure::new(
                    "invalid_input",
                    "boundary.duplicate_binding",
                    "membership batch contains a duplicate mobject binding",
                ));
            }
            match (self.bindings.get(wrapper_id), self.identities.get(&node)) {
                (Some(bound_node), Some(bound_id))
                    if *bound_node == node && *bound_id == *wrapper_id => {}
                (None, None)
                    if matches!(
                        batch.kind,
                        SceneMembershipBatchKind::Add | SceneMembershipBatchKind::Replace
                    ) =>
                {
                    new_bindings.push((*wrapper_id, node));
                }
                _ => {
                    return Err(format!(
                        "canonical object {} has inconsistent membership binding",
                        wrapper_id.get()
                    )
                    .into());
                }
            }
        }
        Ok(new_bindings)
    }

    /// Associate handles already published by an engine-owned lifecycle operation.
    ///
    /// This boundary updates only the language wrapper's derived identity maps.
    /// It neither admits scene members nor publishes an execution transaction.
    /// Validate every reservation against the completed coherent runtime before
    /// committing any mapping; work is limited to the affected handles.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(super) fn associate_published_bindings(
        &mut self,
        batch: SceneMembershipBatch,
    ) -> Result<(), AuthoringFailure> {
        if batch.kind != SceneMembershipBatchKind::Add || !batch.members.is_empty() {
            return Err(AuthoringFailure::new(
                "invalid_input",
                "boundary.association_members",
                "published association accepts binding reservations only",
            ));
        }
        let new_bindings = self.prepare_membership_bindings(&batch)?;
        self.active_live_player()?
            .require_completed_live_segment()?;
        for (_, target) in &batch.bindings {
            if !self.contains_mobject(target)? {
                return Err(AuthoringFailure::new(
                    "invalid_input",
                    "boundary.association_absent",
                    "associated mobject is not in the published Scene",
                ));
            }
            // The shared effective query also rejects stale publications. No
            // handle-only authored fallback or execution rebuild is permitted.
            self.active_live_player()?.live_effective(target)?;
        }
        for (id, node) in new_bindings {
            self.bindings.insert(id, node);
            self.identities.insert(node, id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
