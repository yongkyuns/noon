use std::collections::{HashMap, HashSet};

use super::{
    SemanticNodeId, SemanticNodeKind, SemanticSceneOperationError, SemanticStore,
    SemanticStoreError,
};

#[derive(Debug, Default)]
pub(super) struct FamilyEdgePreflight {
    overrides: HashMap<(SemanticNodeId, SemanticNodeId), bool>,
}

impl FamilyEdgePreflight {
    pub(super) fn add(
        &mut self,
        store: &SemanticStore,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> Result<bool, SemanticSceneOperationError> {
        validate_edge(store, family, member)?;
        if self.contains(store, family, member) {
            return Ok(false);
        }
        if family == member || self.reaches(store, member, family) {
            return Err(SemanticSceneOperationError::Store(
                SemanticStoreError::FamilyCycle { family, member },
            ));
        }
        self.overrides.insert((family, member), true);
        Ok(true)
    }

    pub(super) fn remove(
        &mut self,
        store: &SemanticStore,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> Result<bool, SemanticSceneOperationError> {
        validate_edge(store, family, member)?;
        let changed = self.contains(store, family, member);
        if changed {
            self.overrides.insert((family, member), false);
        }
        Ok(changed)
    }

    fn contains(
        &self,
        store: &SemanticStore,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> bool {
        self.overrides
            .get(&(family, member))
            .copied()
            .unwrap_or_else(|| {
                store
                    .node(family)
                    .is_some_and(|node| node.members().contains(&member))
            })
    }

    fn reaches(
        &self,
        store: &SemanticStore,
        start: SemanticNodeId,
        target: SemanticNodeId,
    ) -> bool {
        let mut stack = vec![start];
        let mut seen = HashSet::new();
        while let Some(current) = stack.pop() {
            if !seen.insert(current) {
                continue;
            }
            if current == target {
                return true;
            }
            stack.extend(self.members(store, current));
        }
        false
    }

    fn members(&self, store: &SemanticStore, family: SemanticNodeId) -> Vec<SemanticNodeId> {
        let mut members = store
            .node(family)
            .map(|node| node.members())
            .unwrap_or_default();
        members.retain(|member| {
            self.overrides
                .get(&(family, *member))
                .copied()
                .unwrap_or(true)
        });
        for ((override_family, member), present) in &self.overrides {
            if *override_family == family && *present && !members.contains(member) {
                members.push(*member);
            }
        }
        members
    }
}

fn validate_edge(
    store: &SemanticStore,
    family: SemanticNodeId,
    member: SemanticNodeId,
) -> Result<(), SemanticSceneOperationError> {
    let family_node = store
        .node(family)
        .ok_or(SemanticSceneOperationError::UnknownNode(family))?;
    if !matches!(family_node.kind(), SemanticNodeKind::Family) {
        return Err(SemanticSceneOperationError::NotSemanticFamily(family));
    }

    let member_node = store
        .node(member)
        .ok_or(SemanticSceneOperationError::UnknownNode(member))?;
    let is_target_authoring_node = match member_node.kind() {
        SemanticNodeKind::Family => true,
        SemanticNodeKind::AuthoringObject => member_node.semantic_object_state().is_some(),
        SemanticNodeKind::Object(_)
        | SemanticNodeKind::Signal(_)
        | SemanticNodeKind::Animation(_) => false,
    };
    if !is_target_authoring_node {
        return Err(SemanticSceneOperationError::NotSemanticAuthoringNode(
            member,
        ));
    }
    Ok(())
}
