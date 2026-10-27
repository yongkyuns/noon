//! Sparse derived dependencies of immutable composed-source execution plans.
//! The object column owns plans; this index contains only existing identities.
use crate::{CompiledScene, ExecutionPatch};
use noon_core::{ComposedSource, ObjectId};
use std::collections::{BTreeMap, BTreeSet};

mod projection;
mod validation;

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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ComposedDependencies {
    owners: BTreeSet<ObjectId>,
    members: BTreeMap<ObjectId, BTreeSet<ObjectId>>,
    children: BTreeMap<ObjectId, BTreeSet<ObjectId>>,
}
impl ComposedDependencies {
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
    fn dependents(&self, member: ObjectId) -> impl Iterator<Item = ObjectId> + '_ {
        self.members.get(&member).into_iter().flatten().copied()
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
}

impl CompiledScene {
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
        let next = self.composed_plan(owner);
        if previous
            .as_ref()
            .zip(next.as_ref())
            .is_some_and(|(old, new)| std::sync::Arc::ptr_eq(old, new))
        {
            return;
        }
        if let Some(old) = previous {
            self.composed_dependencies.remove(owner, &old);
        }
        if let Some(new) = next {
            self.composed_dependencies.insert(owner, &new);
        }
    }
    pub(crate) fn rebuild_composed_dependencies(
        &mut self,
    ) -> Result<(), CompiledComposedSourceError> {
        let mut index = ComposedDependencies::default();
        for row in self.objects.iter().filter(|row| row.live) {
            if let Some(source) = row.content.composed_source() {
                index.insert(row.id, source);
            }
        }
        let view = projection::Projection::with_index(self, &index);
        validation::validate(&view, &index.owners)?;
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
        if self.composed_dependencies.owners.is_empty()
            && !patches.iter().any(projection::introduces_scope)
        {
            return Ok(ComposedValidationStats::default());
        }
        let mut view = projection::Projection::new(self);
        view.stage(patches)
            .map_err(crate::CompilePatchError::InvalidComposedSource)?;
        let affected = view.affected_scopes();
        let mut stats = validation::validate(&view, &affected)
            .map_err(crate::CompilePatchError::InvalidComposedSource)?;
        stats.painter_rows_staged = view.painter_rows_staged();
        Ok(stats)
    }
}

#[cfg(test)]
mod tests;
