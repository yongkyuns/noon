use std::collections::{HashMap, HashSet};

use super::{
    SemanticMutationTransactionError, SemanticSceneOperationError, SemanticStoreError,
    SemanticTransactionNodeRef, TransactionNodeCatalog,
};

#[derive(Debug, Default)]
pub(super) struct FamilyEdgePreflight {
    overrides: HashMap<(SemanticTransactionNodeRef, SemanticTransactionNodeRef), bool>,
    added_order: Vec<(SemanticTransactionNodeRef, SemanticTransactionNodeRef)>,
    events: Vec<FamilyEdgeEvent>,
}

#[derive(Debug)]
enum FamilyEdgeEvent {
    Add(SemanticTransactionNodeRef, SemanticTransactionNodeRef),
    Remove(SemanticTransactionNodeRef, SemanticTransactionNodeRef),
    Reorder {
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
        before: Option<SemanticTransactionNodeRef>,
    },
}

impl FamilyEdgePreflight {
    pub(super) fn add(
        &mut self,
        catalog: &TransactionNodeCatalog<'_>,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
        index: usize,
    ) -> Result<bool, SemanticMutationTransactionError> {
        catalog.ensure_family(family, index)?;
        catalog.ensure_authoring_node(member, index)?;
        if self.contains(catalog, family, member) {
            return Ok(false);
        }
        if family == member || self.reaches(catalog, member, family) {
            return Err(match (family, member) {
                (
                    SemanticTransactionNodeRef::Existing(family),
                    SemanticTransactionNodeRef::Existing(member),
                ) => SemanticMutationTransactionError::Family {
                    index,
                    error: SemanticSceneOperationError::Store(SemanticStoreError::FamilyCycle {
                        family,
                        member,
                    }),
                },
                _ => SemanticMutationTransactionError::PendingFamilyCycle {
                    index,
                    family,
                    member,
                },
            });
        }
        self.overrides.insert((family, member), true);
        self.added_order.push((family, member));
        self.events.push(FamilyEdgeEvent::Add(family, member));
        Ok(true)
    }

    pub(super) fn remove(
        &mut self,
        catalog: &TransactionNodeCatalog<'_>,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
        index: usize,
    ) -> Result<bool, SemanticMutationTransactionError> {
        catalog.ensure_family(family, index)?;
        catalog.ensure_authoring_node(member, index)?;
        let changed = self.contains(catalog, family, member);
        if changed {
            self.overrides.insert((family, member), false);
            self.events.push(FamilyEdgeEvent::Remove(family, member));
        }
        Ok(changed)
    }

    pub(super) fn reorder(
        &mut self,
        catalog: &TransactionNodeCatalog<'_>,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
        before: Option<SemanticTransactionNodeRef>,
        index: usize,
    ) -> Result<bool, SemanticMutationTransactionError> {
        catalog.ensure_family(family, index)?;
        catalog.ensure_authoring_node(member, index)?;
        if !self.contains(catalog, family, member) {
            return Err(self.not_member_error(index, family, member));
        }
        if let Some(anchor) = before {
            catalog.ensure_authoring_node(anchor, index)?;
            if !self.contains(catalog, family, anchor) {
                return Err(self.not_member_error(index, family, anchor));
            }
        }
        let changed = before != Some(member);
        if changed {
            self.events.push(FamilyEdgeEvent::Reorder {
                family,
                member,
                before,
            });
        }
        Ok(changed)
    }

    pub(super) fn members_for_read(
        &self,
        store: &crate::SemanticStore,
        family: SemanticTransactionNodeRef,
    ) -> Vec<SemanticTransactionNodeRef> {
        let mut members = match family {
            SemanticTransactionNodeRef::Existing(family) => store
                .node(family)
                .map(|node| node.members().into_iter().map(Into::into).collect())
                .unwrap_or_default(),
            SemanticTransactionNodeRef::Pending(_) => Vec::new(),
        };
        for event in &self.events {
            match *event {
                FamilyEdgeEvent::Add(event_family, member) if event_family == family => {
                    if !members.contains(&member) {
                        members.push(member);
                    }
                }
                FamilyEdgeEvent::Remove(event_family, member) if event_family == family => {
                    members.retain(|candidate| *candidate != member);
                }
                FamilyEdgeEvent::Reorder {
                    family: event_family,
                    member,
                    before,
                } if event_family == family => {
                    if let Some(position) =
                        members.iter().position(|candidate| *candidate == member)
                    {
                        members.remove(position);
                        let position = before
                            .and_then(|anchor| {
                                members.iter().position(|candidate| *candidate == anchor)
                            })
                            .unwrap_or(members.len());
                        members.insert(position, member);
                    }
                }
                _ => {}
            }
        }
        members
    }

    fn not_member_error(
        &self,
        index: usize,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    ) -> SemanticMutationTransactionError {
        match (family, member) {
            (
                SemanticTransactionNodeRef::Existing(family),
                SemanticTransactionNodeRef::Existing(member),
            ) => SemanticMutationTransactionError::Family {
                index,
                error: SemanticSceneOperationError::Store(SemanticStoreError::NotFamilyMember {
                    family,
                    member,
                }),
            },
            _ => SemanticMutationTransactionError::PendingNotFamilyMember {
                index,
                family,
                member,
            },
        }
    }

    fn contains(
        &self,
        catalog: &TransactionNodeCatalog<'_>,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    ) -> bool {
        self.overrides
            .get(&(family, member))
            .copied()
            .unwrap_or_else(|| catalog.contains(family, member))
    }

    fn reaches(
        &self,
        catalog: &TransactionNodeCatalog<'_>,
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
            stack.extend(self.members(catalog, current));
        }
        false
    }

    fn members(
        &self,
        catalog: &TransactionNodeCatalog<'_>,
        family: SemanticTransactionNodeRef,
    ) -> Vec<SemanticTransactionNodeRef> {
        let mut members = catalog.members(family);
        members.retain(|member| {
            self.overrides
                .get(&(family, *member))
                .copied()
                .unwrap_or(true)
        });
        for (added_family, member) in &self.added_order {
            if *added_family == family
                && self
                    .overrides
                    .get(&(family, *member))
                    .copied()
                    .unwrap_or(false)
                && !members.contains(member)
            {
                members.push(*member);
            }
        }
        members
    }
}
