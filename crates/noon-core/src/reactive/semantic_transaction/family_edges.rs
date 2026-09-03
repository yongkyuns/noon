use std::collections::{HashMap, HashSet};

use super::{
    SemanticNodeId, SemanticNodeKind, SemanticSceneOperationError, SemanticStore,
    SemanticStoreError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProjectedLink {
    previous: Option<SemanticNodeId>,
    next: Option<SemanticNodeId>,
}

#[derive(Debug, Default)]
struct FamilyOrderPreflight {
    ends: HashMap<SemanticNodeId, (Option<SemanticNodeId>, Option<SemanticNodeId>)>,
    links: HashMap<(SemanticNodeId, SemanticNodeId), Option<ProjectedLink>>,
}

impl FamilyOrderPreflight {
    fn ends(
        &mut self,
        store: &SemanticStore,
        family: SemanticNodeId,
    ) -> (Option<SemanticNodeId>, Option<SemanticNodeId>) {
        *self
            .ends
            .entry(family)
            .or_insert_with(|| store.family_order_ends(family).unwrap_or((None, None)))
    }

    fn set_ends(
        &mut self,
        family: SemanticNodeId,
        head: Option<SemanticNodeId>,
        tail: Option<SemanticNodeId>,
    ) {
        self.ends.insert(family, (head, tail));
    }

    fn link(
        &self,
        store: &SemanticStore,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> Option<ProjectedLink> {
        self.links
            .get(&(family, member))
            .copied()
            .unwrap_or_else(|| {
                store
                    .family_member_order_link(family, member)
                    .map(|(previous, next)| ProjectedLink { previous, next })
            })
    }

    fn set_link(
        &mut self,
        family: SemanticNodeId,
        member: SemanticNodeId,
        link: Option<ProjectedLink>,
    ) {
        self.links.insert((family, member), link);
    }

    fn append(&mut self, store: &SemanticStore, family: SemanticNodeId, member: SemanticNodeId) {
        let (mut head, tail) = self.ends(store, family);
        if let Some(tail) = tail {
            let mut tail_link = self
                .link(store, family, tail)
                .expect("projected family tail must have an order link");
            tail_link.next = Some(member);
            self.set_link(family, tail, Some(tail_link));
        } else {
            head = Some(member);
        }
        self.set_link(
            family,
            member,
            Some(ProjectedLink {
                previous: tail,
                next: None,
            }),
        );
        self.set_ends(family, head, Some(member));
    }

    fn remove(&mut self, store: &SemanticStore, family: SemanticNodeId, member: SemanticNodeId) {
        let link = self
            .link(store, family, member)
            .expect("projected family member must have an order link");
        let (mut head, mut tail) = self.ends(store, family);

        if let Some(previous) = link.previous {
            let mut previous_link = self
                .link(store, family, previous)
                .expect("projected family previous member must have an order link");
            previous_link.next = link.next;
            self.set_link(family, previous, Some(previous_link));
        } else {
            head = link.next;
        }
        if let Some(next) = link.next {
            let mut next_link = self
                .link(store, family, next)
                .expect("projected family next member must have an order link");
            next_link.previous = link.previous;
            self.set_link(family, next, Some(next_link));
        } else {
            tail = link.previous;
        }

        self.set_link(family, member, None);
        self.set_ends(family, head, tail);
    }

    fn move_before(
        &mut self,
        store: &SemanticStore,
        family: SemanticNodeId,
        member: SemanticNodeId,
        before: Option<SemanticNodeId>,
    ) -> bool {
        let link = self
            .link(store, family, member)
            .expect("projected family member must have an order link");
        let (_, tail) = self.ends(store, family);
        if before == Some(member)
            || before.is_some_and(|anchor| link.next == Some(anchor))
            || (before.is_none() && tail == Some(member))
        {
            return false;
        }

        self.remove(store, family, member);
        match before {
            Some(anchor) => {
                let anchor_link = self
                    .link(store, family, anchor)
                    .expect("projected family reorder anchor must have an order link");
                let (mut head, tail) = self.ends(store, family);
                if let Some(previous) = anchor_link.previous {
                    let mut previous_link = self
                        .link(store, family, previous)
                        .expect("projected family anchor previous must have an order link");
                    previous_link.next = Some(member);
                    self.set_link(family, previous, Some(previous_link));
                } else {
                    head = Some(member);
                }
                let mut updated_anchor = anchor_link;
                updated_anchor.previous = Some(member);
                self.set_link(family, anchor, Some(updated_anchor));
                self.set_link(
                    family,
                    member,
                    Some(ProjectedLink {
                        previous: anchor_link.previous,
                        next: Some(anchor),
                    }),
                );
                self.set_ends(family, head, tail);
            }
            None => self.append(store, family, member),
        }
        true
    }
}

#[derive(Debug, Default)]
pub(super) struct FamilyEdgePreflight {
    overrides: HashMap<(SemanticNodeId, SemanticNodeId), bool>,
    order: FamilyOrderPreflight,
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
        self.order.append(store, family, member);
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
            self.order.remove(store, family, member);
            self.overrides.insert((family, member), false);
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
            return Err(SemanticSceneOperationError::NotSemanticFamilyMember {
                family,
                member,
            });
        }
        if let Some(anchor) = before {
            validate_edge(store, family, anchor)?;
            if !self.contains(store, family, anchor) {
                return Err(SemanticSceneOperationError::NotSemanticFamilyMember {
                    family,
                    member: anchor,
                });
            }
        }
        Ok(self.order.move_before(store, family, member, before))
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
            .unwrap_or_else(|| store.family_contains_member(family, member))
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
