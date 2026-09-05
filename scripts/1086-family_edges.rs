use std::collections::{HashMap, HashSet};

use super::{
    PendingNodeKind, SemanticNodeKind, SemanticPendingFamilyError, SemanticSceneOperationError,
    SemanticStore, SemanticStoreError, SemanticTransactionNodeRef, SemanticTransactionNodeToken,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FamilyEdgePreflightError {
    Existing(SemanticSceneOperationError),
    Pending(SemanticPendingFamilyError),
}

#[derive(Debug, Default)]
pub(super) struct FamilyEdgePreflight {
    overrides: HashMap<(SemanticTransactionNodeRef, SemanticTransactionNodeRef), bool>,
}

impl FamilyEdgePreflight {
    pub(super) fn add(
        &mut self,
        store: &SemanticStore,
        pending: &HashMap<SemanticTransactionNodeToken, PendingNodeKind>,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    ) -> Result<bool, FamilyEdgePreflightError> {
        validate_edge(store, pending, family, member)?;
        if self.contains(store, family, member) {
            return Ok(false);
        }
        if family == member || self.reaches(store, member, family) {
            return Err(cycle_error(family, member));
        }
        self.overrides.insert((family, member), true);
        Ok(true)
    }

    pub(super) fn remove(
        &mut self,
        store: &SemanticStore,
        pending: &HashMap<SemanticTransactionNodeToken, PendingNodeKind>,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    ) -> Result<bool, FamilyEdgePreflightError> {
        validate_edge(store, pending, family, member)?;
        let changed = self.contains(store, family, member);
        if changed {
            self.overrides.insert((family, member), false);
        }
        Ok(changed)
    }

    pub(super) fn reorder(
        &mut self,
        store: &SemanticStore,
        pending: &HashMap<SemanticTransactionNodeToken, PendingNodeKind>,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
        before: Option<SemanticTransactionNodeRef>,
    ) -> Result<bool, FamilyEdgePreflightError> {
        validate_edge(store, pending, family, member)?;
        if !self.contains(store, family, member) {
            return Err(not_member_error(family, member));
        }
        if let Some(anchor) = before {
            validate_edge(store, pending, family, anchor)?;
            if !self.contains(store, family, anchor) {
                return Err(not_member_error(family, anchor));
            }
        }
        Ok(before != Some(member))
    }

    fn contains(
        &self,
        store: &SemanticStore,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    ) -> bool {
        self.overrides
            .get(&(family, member))
            .copied()
            .unwrap_or_else(|| match (family, member) {
                (
                    SemanticTransactionNodeRef::Existing(family),
                    SemanticTransactionNodeRef::Existing(member),
                ) => store
                    .node(member)
                    .is_some_and(|node| node.parents().contains(&family)),
                _ => false,
            })
    }

    fn reaches(
        &self,
        store: &SemanticStore,
        start: SemanticTransactionNodeRef,
        target: SemanticTransactionNodeRef,
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

    fn members(
        &self,
        store: &SemanticStore,
        family: SemanticTransactionNodeRef,
    ) -> Vec<SemanticTransactionNodeRef> {
        let mut members = match family {
            SemanticTransactionNodeRef::Existing(family) => store
                .node(family)
                .map(|node| {
                    node.members()
                        .into_iter()
                        .map(SemanticTransactionNodeRef::Existing)
                        .collect()
                })
                .unwrap_or_default(),
            SemanticTransactionNodeRef::Pending(_) => Vec::new(),
        };
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
    pending: &HashMap<SemanticTransactionNodeToken, PendingNodeKind>,
    family: SemanticTransactionNodeRef,
    member: SemanticTransactionNodeRef,
) -> Result<(), FamilyEdgePreflightError> {
    match family {
        SemanticTransactionNodeRef::Existing(family) => {
            let family_node = store.node(family).ok_or_else(|| {
                FamilyEdgePreflightError::Existing(SemanticSceneOperationError::UnknownNode(family))
            })?;
            if !matches!(family_node.kind(), SemanticNodeKind::Family) {
                return Err(FamilyEdgePreflightError::Existing(
                    SemanticSceneOperationError::NotSemanticFamily(family),
                ));
            }
        }
        SemanticTransactionNodeRef::Pending(token) => match pending.get(&token) {
            Some(PendingNodeKind::Family) => {}
            Some(PendingNodeKind::Object) => {
                return Err(FamilyEdgePreflightError::Pending(
                    SemanticPendingFamilyError::NotFamily(token),
                ));
            }
            None => {
                return Err(FamilyEdgePreflightError::Pending(
                    SemanticPendingFamilyError::UnknownToken(token),
                ));
            }
        },
    }

    match member {
        SemanticTransactionNodeRef::Pending(token) => {
            if !pending.contains_key(&token) {
                return Err(FamilyEdgePreflightError::Pending(
                    SemanticPendingFamilyError::UnknownToken(token),
                ));
            }
        }
        SemanticTransactionNodeRef::Existing(member) => {
            let member_node = store.node(member).ok_or_else(|| {
                FamilyEdgePreflightError::Existing(SemanticSceneOperationError::UnknownNode(member))
            })?;
            let is_target_authoring_node = match member_node.kind() {
                SemanticNodeKind::Family => true,
                SemanticNodeKind::AuthoringObject => member_node.semantic_object_state().is_some(),
                SemanticNodeKind::Object(_)
                | SemanticNodeKind::Signal(_)
                | SemanticNodeKind::Animation(_) => false,
            };
            if !is_target_authoring_node {
                return Err(FamilyEdgePreflightError::Existing(
                    SemanticSceneOperationError::NotSemanticAuthoringNode(member),
                ));
            }
        }
    }
    Ok(())
}

fn cycle_error(
    family: SemanticTransactionNodeRef,
    member: SemanticTransactionNodeRef,
) -> FamilyEdgePreflightError {
    match (family, member) {
        (
            SemanticTransactionNodeRef::Existing(family),
            SemanticTransactionNodeRef::Existing(member),
        ) => FamilyEdgePreflightError::Existing(SemanticSceneOperationError::Store(
            SemanticStoreError::FamilyCycle { family, member },
        )),
        (family, member) => FamilyEdgePreflightError::Pending(
            SemanticPendingFamilyError::Cycle { family, member },
        ),
    }
}

fn not_member_error(
    family: SemanticTransactionNodeRef,
    member: SemanticTransactionNodeRef,
) -> FamilyEdgePreflightError {
    match (family, member) {
        (
            SemanticTransactionNodeRef::Existing(family),
            SemanticTransactionNodeRef::Existing(member),
        ) => FamilyEdgePreflightError::Existing(SemanticSceneOperationError::Store(
            SemanticStoreError::NotFamilyMember { family, member },
        )),
        (family, member) => FamilyEdgePreflightError::Pending(
            SemanticPendingFamilyError::NotFamilyMember { family, member },
        ),
    }
}
