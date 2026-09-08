use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ops::Bound::{Excluded, Unbounded};

use noon_core::{
    HostCallbackId, PreparedSemanticMutationTransaction, SemanticMutation, SemanticNodeId,
    SemanticNodeKind, SemanticStore, SemanticUpdaterRegistration,
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
    order: usize,
}

impl SemanticHostCallbackOccurrence {
    pub const fn order(self) -> usize {
        self.order
    }

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

impl Eq for SemanticHostCallbackEvent {}

impl Ord for SemanticHostCallbackEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time
            .total_cmp(&other.time)
            .then_with(|| event_kind_order(self.kind).cmp(&event_kind_order(other.kind)))
            .then_with(|| self.occurrence_index.cmp(&other.occurrence_index))
    }
}
impl PartialOrd for SemanticHostCallbackEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Compiler-owned callback data and derived indices. Live changes replace only
/// affected target registrations; unrelated history is neither read nor copied.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticHostCallbackPlan {
    occurrences: BTreeMap<usize, SemanticHostCallbackOccurrence>,
    events: BTreeSet<SemanticHostCallbackEvent>,
    activations: BTreeSet<SemanticHostCallbackEvent>,
    targets: HashMap<SemanticNodeId, (usize, Vec<usize>)>,
    next_index: usize,
}

/// Preflighted target-local compiler delta. Commit only after semantic and runtime
/// validation succeeds. It contains no copy of unrelated callback history.
#[derive(Debug)]
pub struct SemanticHostCallbackRevision {
    targets: Vec<(SemanticNodeId, Vec<SemanticUpdaterRegistration>)>,
}

impl SemanticHostCallbackRevision {
    pub fn targets(&self) -> impl Iterator<Item = SemanticNodeId> + '_ {
        self.targets.iter().map(|(target, _)| *target)
    }
}

impl SemanticHostCallbackPlan {
    pub fn occurrences(&self) -> impl ExactSizeIterator<Item = &SemanticHostCallbackOccurrence> {
        self.occurrences.values()
    }
    pub fn occurrence(&self, index: usize) -> SemanticHostCallbackOccurrence {
        self.occurrences[&index]
    }
    pub fn target_occurrences(&self, target: SemanticNodeId) -> impl Iterator<Item = usize> + '_ {
        self.targets
            .get(&target)
            .into_iter()
            .flat_map(|(_, indices)| indices.iter().copied())
    }
    pub fn events(&self) -> &BTreeSet<SemanticHostCallbackEvent> {
        &self.events
    }
    pub fn is_empty(&self) -> bool {
        self.occurrences.is_empty()
    }

    fn after(time: Option<f64>) -> std::ops::Bound<SemanticHostCallbackEvent> {
        time.map_or(Unbounded, |time| {
            Excluded(SemanticHostCallbackEvent {
                time: if time == 0.0 { 0.0 } else { time },
                occurrence_index: usize::MAX,
                kind: SemanticHostCallbackEventKind::Deactivate,
            })
        })
    }
    pub fn events_after(
        &self,
        time: Option<f64>,
    ) -> impl Iterator<Item = SemanticHostCallbackEvent> + '_ {
        self.events.range((Self::after(time), Unbounded)).copied()
    }
    pub fn next_activation_after(&self, time: Option<f64>) -> Option<f64> {
        self.activations
            .range((Self::after(time), Unbounded))
            .next()
            .map(|event| event.time)
    }

    fn occurrence_events(
        index: usize,
        activation: SemanticUpdaterRegistration,
    ) -> impl Iterator<Item = SemanticHostCallbackEvent> {
        let event = |time: f64, kind| SemanticHostCallbackEvent {
            time: if time == 0.0 { 0.0 } else { time },
            occurrence_index: index,
            kind,
        };
        std::iter::once(event(
            activation.active_from(),
            SemanticHostCallbackEventKind::Activate,
        ))
        .chain(
            activation
                .inactive_from()
                .map(|time| event(time, SemanticHostCallbackEventKind::Deactivate)),
        )
    }

    fn insert(&mut self, target: SemanticNodeId, activation: SemanticUpdaterRegistration) {
        let (order, indices) = self.targets.get_mut(&target).expect("indexed target");
        let index = self.next_index;
        self.next_index += 1;
        indices.push(index);
        self.occurrences.insert(
            index,
            SemanticHostCallbackOccurrence {
                target,
                activation,
                order: *order,
            },
        );
        for event in Self::occurrence_events(index, activation) {
            self.events.insert(event);
            if event.kind == SemanticHostCallbackEventKind::Activate
                && activation
                    .inactive_from()
                    .is_none_or(|end| end > event.time)
            {
                self.activations.insert(event);
            }
        }
    }

    /// Apply a previously prepared delta without scanning dormant history.
    pub fn apply_revision(&mut self, revision: SemanticHostCallbackRevision) {
        for (target, registrations) in revision.targets {
            let indices =
                std::mem::take(&mut self.targets.get_mut(&target).expect("preflighted target").1);
            for index in indices {
                let old = self.occurrences.remove(&index).expect("indexed occurrence");
                for event in Self::occurrence_events(index, old.activation) {
                    self.events.remove(&event);
                    self.activations.remove(&event);
                }
            }
            for registration in registrations {
                self.insert(target, registration);
            }
        }
    }

    /// Prepare only changed target histories, not unrelated callbacks or geometry.
    /// The first live subset retains target preorder from initial lowering and
    /// therefore admits registration edits only on already indexed targets.
    pub(super) fn prepare_registration_revision(
        &self,
        prepared: &PreparedSemanticMutationTransaction<'_>,
        current_time: f64,
    ) -> Result<Option<SemanticHostCallbackRevision>, super::SemanticPublicationLoweringError> {
        use super::SemanticPublicationLoweringError as Error;
        let mut changed = HashSet::new();
        for (index, mutation) in prepared.mutations().iter().enumerate() {
            let (target, boundary) = match mutation {
                SemanticMutation::AddUpdater {
                    target,
                    active_from,
                    ..
                } => (*target, *active_from),
                SemanticMutation::RemoveUpdater {
                    target,
                    inactive_from,
                    ..
                }
                | SemanticMutation::ClearUpdaters {
                    target,
                    inactive_from,
                } => (*target, *inactive_from),
                _ => return Err(Error::UnsupportedMutation { index }),
            };
            if boundary < current_time {
                return Err(Error::RetroactiveUpdaterMutation { index });
            }
            // Exact no-ops must not rebuild the callback index or invalidate an
            // already accepted phase. Staged registrations are semantic-owned.
            let target = target
                .existing()
                .ok_or(Error::UnsupportedMutation { index })?;
            changed.insert(target);
        }
        changed.retain(|&target| {
            prepared
                .proposed_updater_registrations(target)
                .is_some_and(|staged| {
                    staged
                        != prepared
                            .store()
                            .node(target)
                            .expect("validated target")
                            .host_updaters()
                })
        });
        if changed.is_empty() {
            return Ok(None);
        }
        let mut targets = Vec::with_capacity(changed.len());
        for target in changed {
            if !self.targets.contains_key(&target) {
                return Err(Error::UpdaterTargetNotIndexed { target });
            }
            targets.push((
                target,
                prepared
                    .proposed_updater_registrations(target)
                    .expect("changed target has staged registrations")
                    .to_vec(),
            ));
        }
        targets.sort_by_key(|(target, _)| self.targets[target].0);
        Ok(Some(SemanticHostCallbackRevision { targets }))
    }
}

pub(super) fn lower_semantic_host_callbacks(
    store: &SemanticStore,
    roots: &[SemanticNodeId],
) -> SemanticHostCallbackPlan {
    let mut plan = SemanticHostCallbackPlan::default();
    let mut seen = HashSet::new();
    let mut pending = roots.iter().rev().copied().collect::<Vec<_>>();
    while let Some(target) = pending.pop() {
        if !seen.insert(target) {
            continue;
        }
        let node = store
            .node(target)
            .expect("semantic lowering roots and members must remain live");
        if !node.host_updaters().is_empty() {
            let order = plan.targets.len();
            plan.targets.insert(target, (order, Vec::new()));
            for &activation in node.host_updaters() {
                plan.insert(target, activation);
            }
        }
        if matches!(node.kind(), SemanticNodeKind::Family) {
            pending.extend(node.members().into_iter().rev());
        }
    }
    plan
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
        assert_eq!(plan.occurrence(0).target(), selected);
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
    #[test]
    fn staged_updater_revision_uses_semantic_order_and_does_not_publish_early() {
        let mut store = SemanticStore::new();
        let first = object(&mut store, 1.0);
        let second = object(&mut store, 2.0);
        store.attach_to_scene(first).unwrap();
        store.attach_to_scene(second).unwrap();
        add_updater(&mut store, first, 7, 0.0);
        add_updater(&mut store, second, 8, 0.0);
        let original = lower_semantic_host_callbacks(&store, &[first, second]);
        let before = store.scene_revision();
        let mut tx = SemanticMutationTransaction::new();
        tx.remove_updater(first, HostCallbackId::new(7), 2.0);
        tx.add_updater(first, HostCallbackId::new(9), 2.0, None);
        let prepared = tx.prepare(&mut store).unwrap();
        let revised = original
            .prepare_registration_revision(&prepared, 2.0)
            .unwrap()
            .unwrap();
        assert_eq!(prepared.store().scene_revision(), before);
        assert_eq!(
            prepared
                .store()
                .semantic_updater_registrations(first)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(revised.targets.len(), 1);
        assert_eq!(revised.targets[0].0, first);
        assert_eq!(revised.targets[0].1.len(), 2);
        let mut updated = original.clone();
        updated.apply_revision(revised);
        let mut ordered = updated.occurrences().copied().collect::<Vec<_>>();
        ordered.sort_by_key(|item| item.order());
        assert_eq!(
            ordered
                .iter()
                .map(|item| (item.target(), item.callback_id()))
                .collect::<Vec<_>>(),
            vec![
                (first, HostCallbackId::new(7)),
                (first, HostCallbackId::new(9)),
                (second, HostCallbackId::new(8))
            ]
        );
        drop(prepared);
        assert_eq!(store.scene_revision(), before);
        assert_eq!(original.occurrences().len(), 2);
    }
    #[test]
    fn live_registration_delta_keeps_unrelated_callback_history_and_ids() {
        let mut store = SemanticStore::new();
        let roots = (0..1024)
            .map(|_| {
                let target = object(&mut store, 1.0);
                add_updater(&mut store, target, 7, 100.0);
                target
            })
            .collect::<Vec<_>>();
        let mut plan = lower_semantic_host_callbacks(&store, &roots);
        let unrelated_index = plan.target_occurrences(roots[1023]).next().unwrap();
        let unrelated = plan.occurrence(unrelated_index);
        let mut tx = SemanticMutationTransaction::new();
        tx.add_updater(roots[0], HostCallbackId::new(8), 2.0, None);
        let prepared = tx.prepare(&mut store).unwrap();
        let revision = plan
            .prepare_registration_revision(&prepared, 2.0)
            .unwrap()
            .unwrap();
        assert_eq!(revision.targets.len(), 1);
        assert_eq!(
            revision.targets[0].1.len(),
            2,
            "preflight owns only the changed target's history"
        );
        plan.apply_revision(revision);
        assert_eq!(plan.occurrence(unrelated_index), unrelated);
        assert_eq!(plan.next_activation_after(Some(2.0)), Some(100.0));
        assert_eq!(plan.occurrences().len(), 1025);
    }
}
