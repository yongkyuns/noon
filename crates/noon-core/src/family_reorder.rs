use super::{FamilyMemberLink, SemanticNodeId, SemanticNodeKind, SemanticStore};

impl SemanticStore {
    /// O(1) direct family-membership lookup for transaction/shared-operation preflight.
    pub(crate) fn family_contains_member(
        &self,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> bool {
        self.node(family).is_some_and(|node| {
            matches!(node.kind(), SemanticNodeKind::Family) && node.members.contains(member)
        })
    }

    /// Return one direct member's current intrusive-order neighbors in O(1).
    pub(crate) fn family_member_order_link(
        &self,
        family: SemanticNodeId,
        member: SemanticNodeId,
    ) -> Option<(Option<SemanticNodeId>, Option<SemanticNodeId>)> {
        let node = self.node(family)?;
        if !matches!(node.kind(), SemanticNodeKind::Family) {
            return None;
        }
        let link = node.members.links.get(&member)?;
        Some((link.previous, link.next))
    }

    /// Return authoritative family-order endpoints in O(1).
    pub(crate) fn family_order_ends(
        &self,
        family: SemanticNodeId,
    ) -> Option<(Option<SemanticNodeId>, Option<SemanticNodeId>)> {
        let node = self.node(family)?;
        if !matches!(node.kind(), SemanticNodeKind::Family) {
            return None;
        }
        Some((node.members.head, node.members.tail))
    }

    /// Move an existing direct family member immediately before another direct
    /// member, or to the tail when `before` is `None`.
    ///
    /// Validation belongs to the target shared/transaction operation. Once those
    /// identities are validated, the authoritative intrusive order changes only
    /// the family node itself and never scans unrelated siblings.
    pub(crate) fn reorder_member_local(
        &mut self,
        family: SemanticNodeId,
        member: SemanticNodeId,
        before: Option<SemanticNodeId>,
    ) -> bool {
        self.set_last_mutation_writes(0);
        debug_assert!(self.family_contains_member(family, member));
        debug_assert!(before.is_none_or(|anchor| self.family_contains_member(family, anchor)));

        let changed = self
            .node_mut(family)
            .expect("family reorder requires a live family")
            .members
            .move_before(member, before)
            .expect("family reorder endpoints validated before mutation");
        if changed {
            self.set_last_mutation_writes(1);
        }
        changed
    }
}

impl super::OrderedFamilyMembers {
    fn move_before(
        &mut self,
        member: SemanticNodeId,
        before: Option<SemanticNodeId>,
    ) -> Option<bool> {
        let link = self.links.get(&member).copied()?;
        if before == Some(member) {
            return Some(false);
        }
        if let Some(anchor) = before {
            if !self.links.contains_key(&anchor) {
                return None;
            }
            if link.next == Some(anchor) {
                return Some(false);
            }
        } else if self.tail == Some(member) {
            return Some(false);
        }

        if let Some(previous) = link.previous {
            self.links
                .get_mut(&previous)
                .expect("family previous link must exist")
                .next = link.next;
        } else {
            debug_assert_eq!(self.head, Some(member));
            self.head = link.next;
        }
        if let Some(next) = link.next {
            self.links
                .get_mut(&next)
                .expect("family next link must exist")
                .previous = link.previous;
        } else {
            debug_assert_eq!(self.tail, Some(member));
            self.tail = link.previous;
        }

        match before {
            Some(anchor) => {
                let anchor_previous = self
                    .links
                    .get(&anchor)
                    .expect("family reorder anchor must exist")
                    .previous;
                if let Some(previous) = anchor_previous {
                    self.links
                        .get_mut(&previous)
                        .expect("family anchor previous link must exist")
                        .next = Some(member);
                } else {
                    self.head = Some(member);
                }
                self.links
                    .get_mut(&anchor)
                    .expect("family reorder anchor must exist")
                    .previous = Some(member);
                self.links.insert(
                    member,
                    FamilyMemberLink {
                        previous: anchor_previous,
                        next: Some(anchor),
                    },
                );
            }
            None => {
                let previous = self.tail;
                if let Some(previous) = previous {
                    self.links
                        .get_mut(&previous)
                        .expect("family tail link must exist")
                        .next = Some(member);
                } else {
                    self.head = Some(member);
                }
                self.links.insert(
                    member,
                    FamilyMemberLink {
                        previous,
                        next: None,
                    },
                );
                self.tail = Some(member);
            }
        }

        Some(true)
    }
}
