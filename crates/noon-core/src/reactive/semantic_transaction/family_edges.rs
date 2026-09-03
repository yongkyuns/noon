use std::collections::{HashMap, HashSet};

use super::{
    SemanticNodeId, SemanticNodeKind, SemanticSceneOperationError, SemanticStore,
    SemanticStoreError,
};

#[derive(Debug, Default)]
pub(super) struct FamilyEdgePreflight {
    overrides: HashMap<(SemanticNodeId, SemanticNodeId), bool>,
    pending_appends: HashMap<SemanticNodeId, Vec<SemanticNodeId>>,
    orders: HashMap<SemanticNodeId, Vec<SemanticNodeId>>,
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
        if let Some(order) = self.orders.get_mut(&family) {
            order.push(member);
        } else {
            self.pending_appends.entry(family).or_default().push(member);
        }
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
            if let Some(order) = self.orders.get_mut(&family) {
                order.retain(|candidate| *candidate != member);
            }
        }
        Ok(changed)
    }

    pub(super) fn reorder(
        &mut self,
        store: &SemanticStore,
        family: SemanticNodeId,
        member: SemanticNodeId,
        before: Option<SemanticNodeId>,
    ) -> Result<bool, SemanticSceneOperationError> {
        validate_edge(store, family, member)?;
        if !self.contains(store, family, member) {
            return Err(SemanticSceneOperationError::Store(
                SemanticStoreError::NotFamilyMember { family, member },
            ));
        }
        if let Some(anchor) = before {
            validate_edge(store, family, anchor)?;
            if !self.contains(store, family, anchor) {
                return Err(SemanticSceneOperationError::Store(
                    SemanticStoreError::NotFamilyMember {
                        family,
                        member: anchor,
                    },
                ));
            }
        }
        if before == Some(member) {
            return Ok(false);
        }

        self.ensure_order(store, family);
        let order = self
            .orders
            .get_mut(&family)
            .expect("family order initialized above");
        let member_index = order
            .iter()
            .position(|candidate| *candidate == member)
            .expect("preflight membership and order must agree");
        let already_positioned = match before {
            Some(anchor) => order.get(member_index + 1) == Some(&anchor),
            None => member_index + 1 == order.len(),
        };
        if already_positioned {
            return Ok(false);
        }

        order.remove(member_index);
        let insertion_index = match before {
            Some(anchor) => order
                .iter()
                .position(|candidate| *candidate == anchor)
                .expect("preflight anchor membership and order must agree"),
            None => order.len(),
        };
        order.insert(insertion_index, member);
        Ok(true)
    }

    fn ensure_order(&mut self, store: &SemanticStore, family: SemanticNodeId) {
        if self.orders.contains_key(&family) {
            return;
        }
        let mut order = store
            .node(family)
            .map(|node| node.members())
            .unwrap_or_default();
        order.retain(|member| {
            self.overrides
                .get(&(family, *member))
                .copied()
                .unwrap_or(true)
        });
        if let Some(appends) = self.pending_appends.remove(&family) {
            for member in appends {
                if self
                    .overrides
                    .get(&(family, member))
                    .copied()
                    .unwrap_or(false)
                    && !order.contains(&member)
                {
                    order.push(member);
                }
            }
        }
        self.orders.insert(family, order);
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
        if let Some(order) = self.orders.get(&family) {
            return order.clone();
        }
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
        if let Some(appends) = self.pending_appends.get(&family) {
            for member in appends {
                if self
                    .overrides
                    .get(&(family, *member))
                    .copied()
                    .unwrap_or(false)
                    && !members.contains(member)
                {
                    members.push(*member);
                }
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
