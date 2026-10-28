//! Sparse capability/order metadata for the finite planar backdrop profile.
//! Keys are existing physical painter ranks and object identities. No geometry,
//! authored values or alternate ordering authority is retained here.
use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PrefixAdmission {
    inputs: BTreeMap<ObjectId, usize>,
    unsupported: BTreeSet<(usize, ObjectId)>,
    owners: BTreeMap<ObjectId, (ObjectId, Option<usize>)>,
    prefixes: BTreeSet<(usize, ObjectId)>,
    anchors: BTreeMap<ObjectId, BTreeSet<ObjectId>>,
}

/// Sparse final-state metadata, checked before either native or worker commit.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct PrefixAdmissionChanges {
    inputs: BTreeMap<ObjectId, Option<usize>>,
    owners: BTreeMap<ObjectId, Option<(ObjectId, Option<usize>)>>,
    pub rows_examined: usize,
}

/// The initial backdrop profile consumes planar prepared 2D draws. Qualified
/// vector paths carry their immutable path lease inline; unresolved external
/// geometry (including meshes) and spatial draws need a different capture input.
#[doc(hidden)]
pub fn unsupported_prefix_input(
    content: &noon_core::ObjectContentRef,
    spatial: bool,
    draw: bool,
) -> bool {
    draw && (spatial
        || matches!(
            content.geometry(),
            Some(noon_core::GeometryRef::External(_))
        ))
}

impl PrefixAdmission {
    pub(super) fn set_input(&mut self, object: ObjectId, unsupported: bool, rank: Option<usize>) {
        let next = rank.filter(|_| unsupported);
        if self.inputs.get(&object).copied() == next {
            return;
        }
        if let Some(old) = self.inputs.remove(&object) {
            self.unsupported.remove(&(old, object));
        }
        if let Some(rank) = next {
            self.inputs.insert(object, rank);
            self.unsupported.insert((rank, object));
        }
    }
    pub(super) fn set_owner(&mut self, owner: ObjectId, next: Option<(ObjectId, Option<usize>)>) {
        if self.owners.get(&owner).copied() == next {
            return;
        }
        if let Some((anchor, rank)) = self.owners.remove(&owner) {
            if let Some(rank) = rank {
                self.prefixes.remove(&(rank, owner));
            }
            remove_link(&mut self.anchors, anchor, owner);
        }
        if let Some((anchor, rank)) = next {
            self.owners.insert(owner, (anchor, rank));
            self.anchors.entry(anchor).or_default().insert(owner);
            if let Some(rank) = rank {
                self.prefixes.insert((rank, owner));
            }
        }
    }
    pub(super) fn rank(&mut self, object: ObjectId, rank: Option<usize>) {
        if self.inputs.contains_key(&object) {
            self.set_input(object, true, rank);
        }
        let owners = self
            .anchors
            .get(&object)
            .into_iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        for owner in owners {
            self.set_owner(owner, Some((object, rank)));
        }
    }
    fn prepare(
        &self,
        view: &impl ComposedSourceGraphView,
        changed: impl IntoIterator<Item = ObjectId>,
        painter: impl IntoIterator<Item = ObjectId>,
    ) -> PrefixAdmissionChanges {
        let rows = changed.into_iter().chain(painter).collect::<BTreeSet<_>>();
        let mut changes = PrefixAdmissionChanges::default();
        let mut owners = BTreeSet::new();
        for object in rows {
            changes.rows_examined += 1;
            let next = view
                .rank(object)
                .filter(|_| view.prefix_input_unsupported(object));
            if self.inputs.get(&object).copied() != next {
                changes.inputs.insert(object, next);
            }
            if self.owners.contains_key(&object)
                || view.input_role(object) == Some(EffectInputRole::PainterPrefix)
            {
                owners.insert(object);
            }
            owners.extend(self.anchors.get(&object).into_iter().flatten().copied());
        }
        for owner in owners {
            let next =
                (view.input_role(owner) == Some(EffectInputRole::PainterPrefix)).then(|| {
                    let anchor = view
                        .source(owner)
                        .map_or(owner, |source| source.members()[0]);
                    (anchor, view.rank(anchor))
                });
            if self.owners.get(&owner).copied() != next {
                changes.owners.insert(owner, next);
            }
        }
        changes
    }
    fn conflict(&self, changes: &PrefixAdmissionChanges) -> Option<(ObjectId, ObjectId)> {
        let first = self
            .unsupported
            .iter()
            .copied()
            .find(|(_, id)| !changes.inputs.contains_key(id));
        let changed_first = changes
            .inputs
            .iter()
            .filter_map(|(&id, &rank)| rank.map(|rank| (rank, id)))
            .min();
        let first = first.into_iter().chain(changed_first).min()?;
        let last = self
            .prefixes
            .iter()
            .rev()
            .copied()
            .find(|(_, id)| !changes.owners.contains_key(id));
        let changed_last = changes
            .owners
            .iter()
            .filter_map(|(&id, &row)| row.and_then(|(_, rank)| rank.map(|rank| (rank, id))))
            .max();
        let last = last.into_iter().chain(changed_last).max()?;
        (first.0 < last.0).then_some((last.1, first.1))
    }
}

impl ComposedDependencies {
    pub fn prepare_prefix_admission(
        &self,
        view: &impl ComposedSourceGraphView,
        changed: impl IntoIterator<Item = ObjectId>,
        painter: impl IntoIterator<Item = ObjectId>,
    ) -> PrefixAdmissionChanges {
        self.prefix_admission.prepare(view, changed, painter)
    }
    pub fn prefix_conflict(
        &self,
        changes: &PrefixAdmissionChanges,
    ) -> Option<(ObjectId, ObjectId)> {
        self.prefix_admission.conflict(changes)
    }
    pub fn commit_prefix_admission(&mut self, changes: PrefixAdmissionChanges) {
        for (id, rank) in changes.inputs {
            self.prefix_admission.set_input(id, rank.is_some(), rank);
        }
        for (id, next) in changes.owners {
            self.prefix_admission.set_owner(id, next);
        }
    }
}
