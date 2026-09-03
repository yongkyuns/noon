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

#[cfg(test)]
mod tests {
    use crate::{
        SemanticMutationImpact, SemanticMutationTransaction, SemanticMutationTransactionError,
        SemanticNodeId, SemanticObjectState, SemanticSceneOperationError, SemanticSignalSource,
        SemanticSignalValue, SemanticStore, StoredGeometry,
    };

    fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
    }

    fn scalar_input(store: &SemanticStore, signal: SemanticNodeId) -> f64 {
        let SemanticSignalSource::Input(SemanticSignalValue::Scalar(value)) =
            store.semantic_signal_state(signal).unwrap().source()
        else {
            panic!("expected scalar input signal")
        };
        *value
    }

    #[test]
    fn transaction_reorder_shares_atomic_impact_order_with_value_mutations() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.5_f64).unwrap();
        let family = store.insert_family();
        let first = object(&mut store, 1.0);
        let second = object(&mut store, 2.0);
        let third = object(&mut store, 3.0);
        for member in [first, second, third] {
            store.add_semantic_family_member(family, member).unwrap();
        }

        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .set_signal(signal, 0.75_f64)
            .reorder_member(family, third, Some(first));
        let result = transaction.apply(&mut store).unwrap();

        assert_eq!(scalar_input(&store, signal), 0.75);
        assert_eq!(
            store.semantic_family_members_checked(family).unwrap(),
            vec![third, first, second]
        );
        assert_eq!(store.last_mutation_stats().slots_written, 2);
        assert_eq!(
            result.impacts(),
            &[
                SemanticMutationImpact::SignalValue { signal },
                SemanticMutationImpact::FamilyMemberReordered {
                    family,
                    member: third,
                    before: Some(first),
                },
            ]
        );
    }

    #[test]
    fn pending_added_anchor_is_visible_to_later_reorder_preflight() {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        let first = object(&mut store, 1.0);
        let second = object(&mut store, 2.0);
        let anchor = object(&mut store, 3.0);
        store.add_semantic_family_member(family, first).unwrap();
        store.add_semantic_family_member(family, second).unwrap();

        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .add_member(family, anchor)
            .reorder_member(family, first, Some(anchor));
        let result = transaction.apply(&mut store).unwrap();

        assert_eq!(
            store.semantic_family_members_checked(family).unwrap(),
            vec![second, first, anchor]
        );
        assert_eq!(store.last_mutation_stats().slots_written, 2);
        assert_eq!(
            result.impacts(),
            &[
                SemanticMutationImpact::FamilyMemberAdded {
                    family,
                    member: anchor,
                },
                SemanticMutationImpact::FamilyMemberReordered {
                    family,
                    member: first,
                    before: Some(anchor),
                },
            ]
        );
    }

    #[test]
    fn pending_removed_anchor_rejects_reorder_before_any_commit() {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        let first = object(&mut store, 1.0);
        let anchor = object(&mut store, 2.0);
        let third = object(&mut store, 3.0);
        for member in [first, anchor, third] {
            store.add_semantic_family_member(family, member).unwrap();
        }

        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .remove_member(family, anchor)
            .reorder_member(family, third, Some(anchor));

        assert_eq!(
            transaction.apply(&mut store),
            Err(SemanticMutationTransactionError::Family {
                index: 1,
                error: SemanticSceneOperationError::NotSemanticFamilyMember {
                    family,
                    member: anchor,
                },
            })
        );
        assert_eq!(
            store.semantic_family_members_checked(family).unwrap(),
            vec![first, anchor, third]
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn stale_reorder_identity_fails_closed_before_commit() {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        let stale = object(&mut store, 1.0);
        let survivor = object(&mut store, 2.0);
        store.add_semantic_family_member(family, stale).unwrap();
        store.add_semantic_family_member(family, survivor).unwrap();
        store.remove_node(stale).unwrap();
        let replacement = object(&mut store, 3.0);
        assert_eq!(stale.slot(), replacement.slot());
        assert_ne!(stale.generation(), replacement.generation());

        let mut transaction = SemanticMutationTransaction::new();
        transaction.reorder_member(family, stale, Some(survivor));

        assert_eq!(
            transaction.apply(&mut store),
            Err(SemanticMutationTransactionError::Family {
                index: 0,
                error: SemanticSceneOperationError::UnknownNode(stale),
            })
        );
        assert_eq!(
            store.semantic_family_members_checked(family).unwrap(),
            vec![survivor]
        );
        assert_eq!(store.last_mutation_stats().slots_written, 0);
    }

    #[test]
    fn transaction_reorder_is_local_with_ten_thousand_family_members() {
        let mut store = SemanticStore::new();
        let family = store.insert_family();
        let members = (0..10_000)
            .map(|index| object(&mut store, index as f32 + 1.0))
            .collect::<Vec<_>>();
        for member in members.iter().copied() {
            store.add_semantic_family_member(family, member).unwrap();
        }
        let target = members[5_000];
        let anchor = members[10];

        let mut transaction = SemanticMutationTransaction::new();
        transaction.reorder_member(family, target, Some(anchor));
        let result = transaction.apply(&mut store).unwrap();

        let ordered = store.semantic_family_members_checked(family).unwrap();
        assert_eq!(ordered.len(), 10_000);
        assert_eq!(ordered[10], target);
        assert_eq!(ordered[11], anchor);
        assert_eq!(store.last_mutation_stats().slots_written, 1);
        assert_eq!(
            result.impacts(),
            &[SemanticMutationImpact::FamilyMemberReordered {
                family,
                member: target,
                before: Some(anchor),
            }]
        );
    }
}
