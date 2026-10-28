//! Sparse derived dependencies of immutable composed-source execution plans.
//! The object column owns plans; this index contains only existing identities.
use crate::{CompiledScene, ExecutionPatch};
use noon_core::{ComposedSource, EffectInputRole, ObjectId};
use std::collections::{BTreeMap, BTreeSet};

mod prefix_admission;
mod projection;
use prefix_admission::PrefixAdmission;
pub use prefix_admission::{unsupported_prefix_input, PrefixAdmissionChanges};
mod validation;

/// Borrowed final execution state used to validate composed source dependencies.
///
/// The compiler and genuine cross-context receivers supply their existing row,
/// identity and painter-order lookups. Implementations must include staged rows
/// and removals so validation observes the complete proposed transaction.
/// `overlapping` returns every final scope that names the given input.
#[doc(hidden)]
pub trait ComposedSourceGraphView {
    fn source(&self, owner: ObjectId) -> Option<&ComposedSource>;
    fn owner_valid(&self, owner: ObjectId) -> bool;
    fn input_valid(&self, input: ObjectId) -> Option<bool>;
    fn rank(&self, input: ObjectId) -> Option<usize>;
    fn input_role(&self, owner: ObjectId) -> Option<EffectInputRole>;
    fn prefix_input_unsupported(&self, input: ObjectId) -> bool;
    fn prefix_conflict(&self) -> Option<(ObjectId, ObjectId)>;
    fn overlapping(&self, input: ObjectId) -> impl Iterator<Item = ObjectId>;
}

/// Validate affected source scopes against one coherent final execution view.
/// Unchanged scopes must already be valid. Callers include changed scopes,
/// scopes using changed inputs/order, and their final ancestors and children.
#[doc(hidden)]
pub fn validate_composed_sources(
    view: &impl ComposedSourceGraphView,
    affected: &BTreeSet<ObjectId>,
) -> Result<ComposedValidationStats, CompiledComposedSourceError> {
    validation::validate(view, affected)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompiledComposedSourceErrorKind {
    InvalidOwner,
    UnknownInput(ObjectId),
    NonDrawableInput(ObjectId),
    NonContiguousInput,
    InvalidParent(ObjectId),
    ParentCycle,
    CrossingOrAliasedScopes,
    ScopeRoleChanged,
    InvalidBackdropSource,
    UnsupportedBackdropInput(ObjectId),
    NestedBackdrop(ObjectId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledComposedSourceError {
    pub owner: ObjectId,
    pub kind: CompiledComposedSourceErrorKind,
}
impl std::fmt::Display for CompiledComposedSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "composed scope {} is invalid: {:?}",
            self.owner.get(),
            self.kind
        )
    }
}
impl std::error::Error for CompiledComposedSourceError {}

/// Derived reverse links into existing execution plans. This index owns no
/// source plans, execution rows or identities. Update it only when the matching
/// row publication commits; validation can borrow a separate staged index.
#[doc(hidden)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComposedDependencies {
    owners: BTreeSet<ObjectId>,
    prefix_owners: BTreeSet<ObjectId>,
    prefix_admission: PrefixAdmission,
    members: BTreeMap<ObjectId, BTreeSet<ObjectId>>,
    children: BTreeMap<ObjectId, BTreeSet<ObjectId>>,
}
impl ComposedDependencies {
    pub fn is_empty(&self) -> bool {
        self.owners.is_empty() && self.prefix_owners.is_empty()
    }
    pub fn owners(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.owners.iter().copied()
    }
    /// Existing rows whose checked stack consumes the earlier painter prefix.
    pub fn prefix_owners(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.prefix_owners.iter().copied()
    }
    pub fn validation_owners(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.owners.union(&self.prefix_owners).copied()
    }
    pub fn set_input_role(&mut self, owner: ObjectId, role: Option<EffectInputRole>) {
        if role == Some(EffectInputRole::PainterPrefix) {
            self.prefix_owners.insert(owner);
        } else {
            self.prefix_owners.remove(&owner);
        }
    }
    pub fn children(&self, parent: ObjectId) -> impl Iterator<Item = ObjectId> + '_ {
        self.children.get(&parent).into_iter().flatten().copied()
    }
    /// Replace the links for one existing row using its previous and final
    /// immutable plan. Both plans are borrowed from the caller's row storage.
    pub fn replace(
        &mut self,
        owner: ObjectId,
        previous: Option<&ComposedSource>,
        next: Option<&ComposedSource>,
    ) {
        if previous
            .zip(next)
            .is_some_and(|(old, new)| std::ptr::eq(old, new))
            || previous == next
        {
            return;
        }
        if let Some(previous) = previous {
            self.remove(owner, previous);
        }
        if let Some(next) = next {
            self.insert(owner, next);
        }
    }
    fn insert(&mut self, owner: ObjectId, source: &ComposedSource) {
        self.owners.insert(owner);
        for &member in source.members() {
            self.members.entry(member).or_default().insert(owner);
        }
        if let Some(parent) = source.parent_scope() {
            self.children.entry(parent).or_default().insert(owner);
        }
    }
    fn remove(&mut self, owner: ObjectId, source: &ComposedSource) {
        self.owners.remove(&owner);
        for &member in source.members() {
            remove_link(&mut self.members, member, owner);
        }
        if let Some(parent) = source.parent_scope() {
            remove_link(&mut self.children, parent, owner);
        }
    }
    pub fn dependents(&self, member: ObjectId) -> impl Iterator<Item = ObjectId> + '_ {
        self.members.get(&member).into_iter().flatten().copied()
    }
}

/// Sparse derived dependency overlay for a proposed execution publication.
/// Source payloads stay borrowed from the existing rows and their staged edits.
#[doc(hidden)]
pub struct ComposedDependencyChanges<'a> {
    base: &'a ComposedDependencies,
    added: ComposedDependencies,
    replaced: BTreeSet<ObjectId>,
}
impl<'a> ComposedDependencyChanges<'a> {
    pub fn new(base: &'a ComposedDependencies) -> Self {
        Self {
            base,
            added: ComposedDependencies::default(),
            replaced: BTreeSet::new(),
        }
    }
    /// Stage a row's final source links. On repeated staging, `previous` is the
    /// preceding staged plan, not the installed plan.
    pub fn replace(
        &mut self,
        owner: ObjectId,
        previous: Option<&ComposedSource>,
        next: Option<&ComposedSource>,
    ) {
        if self.replaced.insert(owner) {
            self.added.replace(owner, None, next);
        } else {
            self.added.replace(owner, previous, next);
        }
    }
    pub fn prepare_prefix_admission(
        &self,
        view: &impl ComposedSourceGraphView,
        changed: impl IntoIterator<Item = ObjectId>,
        painter: impl IntoIterator<Item = ObjectId>,
    ) -> PrefixAdmissionChanges {
        self.base.prepare_prefix_admission(view, changed, painter)
    }
    pub fn prefix_conflict(
        &self,
        changes: &PrefixAdmissionChanges,
    ) -> Option<(ObjectId, ObjectId)> {
        self.base.prefix_conflict(changes)
    }
    pub fn overlapping(&self, member: ObjectId) -> impl Iterator<Item = ObjectId> + '_ {
        self.base
            .dependents(member)
            .filter(|owner| !self.replaced.contains(owner))
            .chain(self.added.dependents(member))
    }
    /// Find the local validation closure of changed row properties, source
    /// links and painter ranks. The view supplies the complete proposed state.
    pub fn affected_scopes(
        &self,
        view: &impl ComposedSourceGraphView,
        changed: impl IntoIterator<Item = ObjectId>,
        painter_rows: impl IntoIterator<Item = ObjectId>,
    ) -> BTreeSet<ObjectId> {
        let mut result = BTreeSet::new();
        for id in changed.into_iter().chain(self.replaced.iter().copied()) {
            if view.source(id).is_some()
                || view.input_role(id) == Some(EffectInputRole::PainterPrefix)
            {
                result.insert(id);
            }
            result.extend(self.base.dependents(id));
            result.extend(self.added.dependents(id));
            result.extend(self.base.children(id));
        }
        // Controller ranks are metadata order only. A shifted unrelated
        // trailing controller does not make its entire input list dirty.
        for id in painter_rows {
            result.extend(self.base.dependents(id));
            result.extend(self.added.dependents(id));
        }
        for member in self.added.members.keys() {
            result.extend(self.base.dependents(*member));
        }
        let mut pending = result.iter().copied().collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            if let Some(parent) = view.source(id).and_then(ComposedSource::parent_scope) {
                if result.insert(parent) {
                    pending.push(parent);
                }
            }
            for child in self.base.children(id).chain(self.added.children(id)) {
                if view
                    .source(child)
                    .is_some_and(|source| source.parent_scope() == Some(id))
                    && result.insert(child)
                {
                    pending.push(child);
                }
            }
        }
        result
    }
}
fn remove_link(index: &mut BTreeMap<ObjectId, BTreeSet<ObjectId>>, key: ObjectId, owner: ObjectId) {
    if let Some(values) = index.get_mut(&key) {
        values.remove(&owner);
        if values.is_empty() {
            index.remove(&key);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComposedValidationStats {
    pub scopes_validated: usize,
    pub members_examined: usize,
    /// Only shifted positions are staged; existing scene payloads are borrowed.
    pub painter_rows_staged: usize,
    pub prefix_rows_examined: usize,
}

impl CompiledScene {
    #[doc(hidden)]
    pub fn has_painter_prefix_sources(&self) -> bool {
        !self.composed_dependencies.prefix_owners.is_empty()
    }

    pub(crate) fn composed_plan(&self, owner: ObjectId) -> Option<std::sync::Arc<ComposedSource>> {
        let row = &self.objects[self.object_index(owner)? as usize];
        if let noon_core::ObjectContentRef::Composed(source) = &row.content {
            Some(source.clone())
        } else {
            None
        }
    }
    pub(crate) fn update_composed_dependency(
        &mut self,
        owner: ObjectId,
        previous: Option<std::sync::Arc<ComposedSource>>,
    ) {
        let role = self.object_index(owner).map(|index| {
            self.objects[index as usize]
                .effects
                .as_ref()
                .map_or(EffectInputRole::Painted, |stack| stack.input_role())
        });
        self.composed_dependencies.set_input_role(owner, role);
        self.refresh_prefix_admission(owner);
        let next = self.composed_plan(owner);
        if previous
            .as_ref()
            .zip(next.as_ref())
            .is_some_and(|(old, new)| std::sync::Arc::ptr_eq(old, new))
        {
            return;
        }
        self.composed_dependencies
            .replace(owner, previous.as_deref(), next.as_deref());
    }
    pub(crate) fn refresh_prefix_admission(&mut self, object: ObjectId) {
        let index = self.object_index(object);
        let rank = index
            .and_then(|index| self.painter_rank(index))
            .map(|rank| rank as usize);
        let unsupported = index.is_some_and(|index| {
            let row = &self.objects[index as usize];
            unsupported_prefix_input(&row.content, row.spatial.is_some(), row.draw_enabled)
        });
        let owner = index.and_then(|index| {
            let row = &self.objects[index as usize];
            row.effects
                .as_ref()
                .filter(|stack| stack.input_role() == EffectInputRole::PainterPrefix)
                .map(|_| {
                    let anchor = row
                        .content
                        .composed_source()
                        .map_or(object, |source| source.members()[0]);
                    let rank = self
                        .object_index(anchor)
                        .and_then(|i| self.painter_rank(i))
                        .map(|rank| rank as usize);
                    (anchor, rank)
                })
        });
        self.composed_dependencies
            .prefix_admission
            .set_input(object, unsupported, rank);
        self.composed_dependencies
            .prefix_admission
            .set_owner(object, owner);
        self.composed_dependencies
            .prefix_admission
            .rank(object, rank);
    }
    pub(crate) fn refresh_prefix_rank(&mut self, object: ObjectId, rank: Option<usize>) {
        self.composed_dependencies
            .prefix_admission
            .rank(object, rank);
    }
    pub(crate) fn rebuild_composed_dependencies(
        &mut self,
    ) -> Result<(), CompiledComposedSourceError> {
        let mut index = ComposedDependencies::default();
        for row in self.objects.iter().filter(|row| row.live) {
            index.set_input_role(
                row.id,
                Some(
                    row.effects
                        .as_ref()
                        .map_or(EffectInputRole::Painted, |stack| stack.input_role()),
                ),
            );
            if let Some(source) = row.content.composed_source() {
                index.insert(row.id, source);
            }
        }
        for row in self.objects.iter().filter(|row| row.live) {
            let rank = self
                .object_index(row.id)
                .and_then(|i| self.painter_rank(i))
                .map(|rank| rank as usize);
            index.prefix_admission.set_input(
                row.id,
                unsupported_prefix_input(&row.content, row.spatial.is_some(), row.draw_enabled),
                rank,
            );
            if row
                .effects
                .as_ref()
                .is_some_and(|stack| stack.input_role() == EffectInputRole::PainterPrefix)
            {
                let anchor = row
                    .content
                    .composed_source()
                    .map_or(row.id, |source| source.members()[0]);
                let rank = self
                    .object_index(anchor)
                    .and_then(|i| self.painter_rank(i))
                    .map(|rank| rank as usize);
                index
                    .prefix_admission
                    .set_owner(row.id, Some((anchor, rank)));
            }
        }
        let view = projection::Projection::with_index(self, &index);
        validate_composed_sources(&view, &index.validation_owners().collect())?;
        self.composed_dependencies = index;
        Ok(())
    }

    /// Validate only source dependencies affected by these existing mutations.
    /// The complete final transaction is checked before any runtime write.
    #[doc(hidden)]
    pub fn preflight_composed_mutations(
        &self,
        patches: &[ExecutionPatch],
    ) -> Result<ComposedValidationStats, crate::CompilePatchError> {
        if !patches.iter().any(projection::affects_source_graph) {
            return Ok(ComposedValidationStats::default());
        }
        if self.composed_dependencies.is_empty()
            && !patches.iter().any(projection::introduces_scope)
        {
            return Ok(ComposedValidationStats::default());
        }
        let mut view = projection::Projection::new(self);
        view.stage(patches)
            .map_err(crate::CompilePatchError::InvalidComposedSource)?;
        let affected = view.affected_scopes();
        let mut stats = validate_composed_sources(&view, &affected)
            .map_err(crate::CompilePatchError::InvalidComposedSource)?;
        stats.painter_rows_staged = view.painter_rows_staged();
        stats.prefix_rows_examined = view.prefix_rows_examined();
        Ok(stats)
    }
}

#[cfg(test)]
mod tests;
