use std::collections::HashSet;

use noon_core::{
    HostCallbackId, SemanticNodeId, SemanticNodeKind, SemanticStore, SemanticUpdaterRegistration,
};

/// One semantic registration occurrence in deterministic authoring order.
///
/// Callable identity remains host-owned. Repeated `callback_id` values are valid:
/// the occurrence's derived index and semantic target distinguish registrations
/// without allocating another semantic identity domain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticHostCallbackOccurrence {
    target: SemanticNodeId,
    activation: SemanticUpdaterRegistration,
}

impl SemanticHostCallbackOccurrence {
    pub const fn callback_id(self) -> HostCallbackId {
        self.activation.callback()
    }

    pub const fn target(self) -> SemanticNodeId {
        self.target
    }

    pub const fn activation(self) -> SemanticUpdaterRegistration {
        self.activation
    }
}

/// Change to active callback membership at one authored time boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticHostCallbackEventKind {
    Activate,
    Deactivate,
}

/// Preindexed event for an occurrence in [`SemanticHostCallbackPlan::occurrences`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticHostCallbackEvent {
    time: f64,
    occurrence_index: usize,
    kind: SemanticHostCallbackEventKind,
}

impl SemanticHostCallbackEvent {
    pub const fn time(self) -> f64 {
        self.time
    }

    pub const fn occurrence_index(self) -> usize {
        self.occurrence_index
    }

    pub const fn kind(self) -> SemanticHostCallbackEventKind {
        self.kind
    }
}

/// Compiler-owned schedule for semantic host callback occurrences.
///
/// Events are sorted once during lowering. Runtime selection can advance across
/// crossed boundaries and maintain its ordered active set without scanning dormant
/// registration history on every frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticHostCallbackPlan {
    occurrences: Vec<SemanticHostCallbackOccurrence>,
    events: Vec<SemanticHostCallbackEvent>,
}

impl SemanticHostCallbackPlan {
    pub fn occurrences(&self) -> &[SemanticHostCallbackOccurrence] {
        &self.occurrences
    }

    pub fn events(&self) -> &[SemanticHostCallbackEvent] {
        &self.events
    }

    pub fn is_empty(&self) -> bool {
        self.occurrences.is_empty()
    }
}

pub(super) fn lower_semantic_host_callbacks(
    store: &SemanticStore,
    roots: &[SemanticNodeId],
) -> SemanticHostCallbackPlan {
    let mut occurrences = Vec::new();
    let mut seen = HashSet::new();
    let mut pending = roots.iter().rev().copied().collect::<Vec<_>>();
    while let Some(target) = pending.pop() {
        if !seen.insert(target) {
            continue;
        }
        let node = store
            .node(target)
            .expect("semantic lowering roots and members must remain live");
        occurrences.extend(
            node.host_updaters()
                .iter()
                .copied()
                .map(|activation| SemanticHostCallbackOccurrence { target, activation }),
        );
        if matches!(node.kind(), SemanticNodeKind::Family) {
            pending.extend(node.members().into_iter().rev());
        }
    }

    let mut events = Vec::with_capacity(occurrences.len().saturating_mul(2));
    for (occurrence_index, occurrence) in occurrences.iter().enumerate() {
        let activation = occurrence.activation();
        events.push(SemanticHostCallbackEvent {
            time: activation.active_from(),
            occurrence_index,
            kind: SemanticHostCallbackEventKind::Activate,
        });
        if let Some(time) = activation.inactive_from() {
            events.push(SemanticHostCallbackEvent {
                time,
                occurrence_index,
                kind: SemanticHostCallbackEventKind::Deactivate,
            });
        }
    }
    events.sort_by(|left, right| {
        left.time
            .total_cmp(&right.time)
            .then_with(|| event_kind_order(left.kind).cmp(&event_kind_order(right.kind)))
            .then_with(|| left.occurrence_index.cmp(&right.occurrence_index))
    });

    SemanticHostCallbackPlan {
        occurrences,
        events,
    }
}

const fn event_kind_order(kind: SemanticHostCallbackEventKind) -> u8 {
    match kind {
        // At a zero-width interval, activation is immediately followed by
        // deactivation so the interval remains empty under [start, end) rules.
        SemanticHostCallbackEventKind::Activate => 0,
        SemanticHostCallbackEventKind::Deactivate => 1,
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{SemanticMutationTransaction, SemanticObjectState, StoredGeometry};

    use super::*;
    use crate::SemanticExecutionIndex;

    fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
    }

    fn add_updater(
        store: &mut SemanticStore,
        target: SemanticNodeId,
        callback: u64,
        active_from: f64,
    ) {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(target, HostCallbackId::new(callback), active_from, None);
        transaction.apply(store).unwrap();
    }

    #[test]
    fn lowering_preserves_preorder_and_deduplicates_family_aliases() {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let nested = store.insert_family();
        let first = object(&mut store, 1.0);
        let second = object(&mut store, 2.0);
        store.add_semantic_family_member(root, first).unwrap();
        store.add_semantic_family_member(root, nested).unwrap();
        store.add_semantic_family_member(nested, second).unwrap();
        store.add_semantic_family_member(nested, first).unwrap();
        store.attach_to_scene(root).unwrap();

        add_updater(&mut store, root, 1, 0.0);
        add_updater(&mut store, first, 2, 0.0);
        add_updater(&mut store, nested, 3, 0.0);
        add_updater(&mut store, first, 2, 1.0);

        let mut index = SemanticExecutionIndex::new();
        let lowered = crate::lower_semantic_execution(&store, &mut index).unwrap();
        let plan = lowered.host_callbacks();

        assert_eq!(
            plan.occurrences()
                .iter()
                .map(|occurrence| (occurrence.target(), occurrence.callback_id()))
                .collect::<Vec<_>>(),
            vec![
                (root, HostCallbackId::new(1)),
                (first, HostCallbackId::new(2)),
                (first, HostCallbackId::new(2)),
                (nested, HostCallbackId::new(3)),
            ]
        );
    }

    #[test]
    fn event_index_uses_inclusive_start_and_exclusive_end_boundaries() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        store.attach_to_scene(target).unwrap();
        add_updater(&mut store, target, 7, 0.0);

        let mut close_first = SemanticMutationTransaction::new();
        close_first.remove_updater(target, HostCallbackId::new(7), 1.0);
        close_first.apply(&mut store).unwrap();
        add_updater(&mut store, target, 8, 1.0);
        let mut close_zero_width = SemanticMutationTransaction::new();
        close_zero_width.remove_updater(target, HostCallbackId::new(8), 1.0);
        close_zero_width.apply(&mut store).unwrap();

        let mut index = SemanticExecutionIndex::new();
        let lowered = crate::lower_semantic_execution(&store, &mut index).unwrap();
        let plan = lowered.host_callbacks();
        assert_eq!(
            plan.events()
                .iter()
                .map(|event| (event.time(), event.occurrence_index(), event.kind()))
                .collect::<Vec<_>>(),
            vec![
                (0.0, 0, SemanticHostCallbackEventKind::Activate),
                (1.0, 1, SemanticHostCallbackEventKind::Activate),
                (1.0, 0, SemanticHostCallbackEventKind::Deactivate),
                (1.0, 1, SemanticHostCallbackEventKind::Deactivate),
            ]
        );
    }

    #[test]
    fn scoped_lowering_excludes_unrelated_registration_history() {
        let mut store = SemanticStore::new();
        let selected = store.insert_family();
        let unrelated = store.insert_family();
        let selected_object = object(&mut store, 1.0);
        let unrelated_object = object(&mut store, 2.0);
        store
            .add_semantic_family_member(selected, selected_object)
            .unwrap();
        store
            .add_semantic_family_member(unrelated, unrelated_object)
            .unwrap();
        add_updater(&mut store, selected, 1, 0.0);
        add_updater(&mut store, unrelated, 2, 0.0);

        let mut index = SemanticExecutionIndex::new();
        let lowered = crate::lower_semantic_execution_root(&store, selected, &mut index).unwrap();
        let plan = lowered.host_callbacks();

        assert_eq!(plan.occurrences().len(), 1);
        assert_eq!(plan.occurrences()[0].target(), selected);
        assert!(index.execution_object_id(selected_object).is_some());
    }

    #[test]
    fn live_registration_relowering_is_explicitly_unsupported() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(target, HostCallbackId::new(1), 0.0, None);

        assert_eq!(
            crate::validate_semantic_publication(&transaction),
            Err(crate::SemanticPublicationLoweringError::UnsupportedMutation { index: 0 })
        );
        assert!(store
            .semantic_updater_registrations(target)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn no_callback_scene_has_empty_schedule() {
        let mut store = SemanticStore::new();
        let target = object(&mut store, 1.0);
        store.attach_to_scene(target).unwrap();

        let lowered =
            crate::lower_semantic_execution(&store, &mut SemanticExecutionIndex::new()).unwrap();
        assert!(lowered.host_callbacks().is_empty());
    }
}
