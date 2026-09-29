use std::collections::{HashMap, HashSet};

use super::{
    SemanticMutationTransactionError, SemanticSceneOperationError, SemanticStoreError,
    SemanticTransactionNodeRef, TransactionNodeCatalog,
};

#[derive(Debug, Default)]
pub(super) struct FamilyEdgePreflight {
    overrides: HashMap<(SemanticTransactionNodeRef, SemanticTransactionNodeRef), bool>,
    // Cycle and detachment queries inspect only this node's staged adjacency,
    // never all earlier edges in a wide authored batch.
    added_members: HashMap<SemanticTransactionNodeRef, Vec<SemanticTransactionNodeRef>>,
    added_parents: HashMap<SemanticTransactionNodeRef, Vec<SemanticTransactionNodeRef>>,
    order: FamilyOrderOverlay,
    events: Vec<FamilyEdgeEvent>,
}

/// Sparse ordered-edge overlay.  Only endpoints touched by a staged edge or
/// reorder are retained; untouched neighbors continue to use the store's
/// intrusive family links.
#[derive(Debug, Default)]
struct FamilyOrderOverlay {
    links: HashMap<(SemanticTransactionNodeRef, SemanticTransactionNodeRef), FamilyOrderLink>,
    first: HashMap<SemanticTransactionNodeRef, Option<SemanticTransactionNodeRef>>,
    last: HashMap<SemanticTransactionNodeRef, Option<SemanticTransactionNodeRef>>,
}

#[derive(Clone, Copy, Debug, Default)]
struct FamilyOrderLink {
    previous: Option<SemanticTransactionNodeRef>,
    next: Option<SemanticTransactionNodeRef>,
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
    pub(super) fn is_detached(
        &self,
        catalog: &TransactionNodeCatalog<'_>,
        member: SemanticTransactionNodeRef,
    ) -> bool {
        if let SemanticTransactionNodeRef::Existing(id) = member {
            if catalog.existing_node_is_scene_owned_or_parented(id, |family| {
                self.overrides
                    .get(&(family.into(), member))
                    .copied()
                    .unwrap_or(true)
            }) {
                return false;
            }
        }
        !self.added_parents.get(&member).is_some_and(|parents| {
            parents
                .iter()
                .any(|family| self.contains(catalog, *family, member))
        })
    }

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
        self.added_members.entry(family).or_default().push(member);
        self.added_parents.entry(member).or_default().push(family);
        self.order.append(catalog, family, member);
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
            self.order.detach(catalog, family, member);
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
            self.order.detach(catalog, family, member);
            self.order.insert_before(catalog, family, member, before);
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

    pub(super) fn contains(
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

    pub(super) fn added_parents(
        &self,
        catalog: &TransactionNodeCatalog<'_>,
        member: SemanticTransactionNodeRef,
    ) -> impl Iterator<Item = SemanticTransactionNodeRef> + '_ {
        self.added_parents
            .get(&member)
            .into_iter()
            .flatten()
            .copied()
            .filter(move |family| self.contains(catalog, *family, member))
    }

    pub(super) fn contains_existing(
        &self,
        store: &crate::SemanticStore,
        family: crate::SemanticNodeId,
        member: crate::SemanticNodeId,
    ) -> bool {
        self.overrides
            .get(&(family.into(), member.into()))
            .copied()
            .unwrap_or_else(|| {
                store
                    .node(family)
                    .is_some_and(|node| node.contains_member(member))
            })
    }

    pub(super) fn first_existing(
        &self,
        store: &crate::SemanticStore,
        family: crate::SemanticNodeId,
    ) -> Option<crate::SemanticNodeId> {
        self.order.first_existing(store, family)
    }
    pub(super) fn next_existing(
        &self,
        store: &crate::SemanticStore,
        family: crate::SemanticNodeId,
        member: crate::SemanticNodeId,
    ) -> Option<crate::SemanticNodeId> {
        self.order.next_existing(store, family, member)
    }
    pub(super) fn previous_existing(
        &self,
        store: &crate::SemanticStore,
        family: crate::SemanticNodeId,
        member: crate::SemanticNodeId,
    ) -> Option<crate::SemanticNodeId> {
        self.order.previous_existing(store, family, member)
    }
    pub(super) fn added_parents_existing(
        &self,
        member: crate::SemanticNodeId,
    ) -> impl Iterator<Item = crate::SemanticNodeId> + '_ {
        self.added_parents
            .get(&member.into())
            .into_iter()
            .flatten()
            .filter_map(|family| {
                let family = family.existing()?;
                self.contains_existing_placeholder(family, member)
                    .then_some(family)
            })
    }

    fn contains_existing_placeholder(
        &self,
        family: crate::SemanticNodeId,
        member: crate::SemanticNodeId,
    ) -> bool {
        self.overrides
            .get(&(family.into(), member.into()))
            .copied()
            .unwrap_or(false)
    }

    pub(super) fn first_member(
        &self,
        catalog: &TransactionNodeCatalog<'_>,
        family: SemanticTransactionNodeRef,
    ) -> Option<SemanticTransactionNodeRef> {
        self.order.first(catalog, family)
    }

    pub(super) fn next_member(
        &self,
        catalog: &TransactionNodeCatalog<'_>,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    ) -> Option<SemanticTransactionNodeRef> {
        self.order.next(catalog, family, member)
    }

    pub(super) fn previous_member(
        &self,
        catalog: &TransactionNodeCatalog<'_>,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    ) -> Option<SemanticTransactionNodeRef> {
        self.order.previous(catalog, family, member)
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
        for member in self.added_members.get(&family).into_iter().flatten() {
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
        members
    }
}

impl FamilyOrderOverlay {
    fn first_existing(
        &self,
        store: &crate::SemanticStore,
        family: crate::SemanticNodeId,
    ) -> Option<crate::SemanticNodeId> {
        match self.first.get(&family.into()) {
            Some(member) => member.and_then(|member| member.existing()),
            None => store.node(family).and_then(|node| node.first_member()),
        }
    }
    fn next_existing(
        &self,
        store: &crate::SemanticStore,
        family: crate::SemanticNodeId,
        member: crate::SemanticNodeId,
    ) -> Option<crate::SemanticNodeId> {
        match self.links.get(&(family.into(), member.into())) {
            Some(link) => link.next.and_then(|node| node.existing()),
            None => store.node(family).and_then(|node| node.next_member(member)),
        }
    }
    fn previous_existing(
        &self,
        store: &crate::SemanticStore,
        family: crate::SemanticNodeId,
        member: crate::SemanticNodeId,
    ) -> Option<crate::SemanticNodeId> {
        match self.links.get(&(family.into(), member.into())) {
            Some(link) => link.previous.and_then(|node| node.existing()),
            None => store
                .node(family)
                .and_then(|node| node.previous_member(member)),
        }
    }
    fn first(
        &self,
        catalog: &TransactionNodeCatalog<'_>,
        family: SemanticTransactionNodeRef,
    ) -> Option<SemanticTransactionNodeRef> {
        self.first
            .get(&family)
            .copied()
            .unwrap_or_else(|| catalog.first_member(family))
    }

    fn last(
        &self,
        catalog: &TransactionNodeCatalog<'_>,
        family: SemanticTransactionNodeRef,
    ) -> Option<SemanticTransactionNodeRef> {
        self.last
            .get(&family)
            .copied()
            .unwrap_or_else(|| catalog.last_member(family))
    }

    fn next(
        &self,
        catalog: &TransactionNodeCatalog<'_>,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    ) -> Option<SemanticTransactionNodeRef> {
        self.links
            .get(&(family, member))
            .map(|link| link.next)
            .unwrap_or_else(|| catalog.next_member(family, member))
    }

    fn previous(
        &self,
        catalog: &TransactionNodeCatalog<'_>,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    ) -> Option<SemanticTransactionNodeRef> {
        self.links
            .get(&(family, member))
            .map(|link| link.previous)
            .unwrap_or_else(|| catalog.previous_member(family, member))
    }

    fn append(
        &mut self,
        catalog: &TransactionNodeCatalog<'_>,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    ) {
        let previous = self.last(catalog, family);
        if let Some(previous) = previous {
            self.link_mut(catalog, family, previous).next = Some(member);
        } else {
            self.first.insert(family, Some(member));
        }
        self.link_mut(catalog, family, member).previous = previous;
        self.link_mut(catalog, family, member).next = None;
        self.last.insert(family, Some(member));
    }

    fn detach(
        &mut self,
        catalog: &TransactionNodeCatalog<'_>,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    ) {
        let previous = self.previous(catalog, family, member);
        let next = self.next(catalog, family, member);
        if let Some(previous) = previous {
            self.link_mut(catalog, family, previous).next = next;
        } else {
            self.first.insert(family, next);
        }
        if let Some(next) = next {
            self.link_mut(catalog, family, next).previous = previous;
        } else {
            self.last.insert(family, previous);
        }
        let link = self.link_mut(catalog, family, member);
        link.previous = None;
        link.next = None;
    }

    fn insert_before(
        &mut self,
        catalog: &TransactionNodeCatalog<'_>,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
        before: Option<SemanticTransactionNodeRef>,
    ) {
        let Some(before) = before else {
            self.append(catalog, family, member);
            return;
        };
        let previous = self.previous(catalog, family, before);
        if let Some(previous) = previous {
            self.link_mut(catalog, family, previous).next = Some(member);
        } else {
            self.first.insert(family, Some(member));
        }
        self.link_mut(catalog, family, before).previous = Some(member);
        let link = self.link_mut(catalog, family, member);
        link.previous = previous;
        link.next = Some(before);
    }

    fn link_mut(
        &mut self,
        catalog: &TransactionNodeCatalog<'_>,
        family: SemanticTransactionNodeRef,
        member: SemanticTransactionNodeRef,
    ) -> &mut FamilyOrderLink {
        self.links
            .entry((family, member))
            .or_insert_with(|| FamilyOrderLink {
                previous: catalog.previous_member(family, member),
                next: catalog.next_member(family, member),
            })
    }
}
