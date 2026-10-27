//! Disposable dependency and placement indices for the first flat-scope profile.
//! Values and membership plans remain borrowed from execution publications.
use super::*;

#[derive(Debug)]
pub(super) struct ComposedCapture {
    pub(super) owner: ObjectId,
    pub(super) source: Arc<noon_core::ComposedSource>,
    members: Vec<usize>,
}

#[derive(Debug, Default)]
pub(super) struct ComposedCaptures {
    pub(super) scopes: BTreeMap<usize, ComposedCapture>,
    dependents: BTreeMap<usize, (usize, ObjectId)>,
    placements: BTreeMap<ObjectId, usize>,
}

impl ComposedCaptures {
    pub(super) fn install(
        &mut self,
        publication: &RendererPublication<'_>,
        ranks: &[usize],
        index: usize,
    ) -> Result<(), EffectPrepareError> {
        let row = &publication.frame().objects[index];
        let ObjectContentRef::Composed(source) = &row.content else {
            self.remove(index);
            return Ok(());
        };
        if row.effects.is_none() {
            self.remove(index);
            return Ok(());
        }
        // #1897 owns qualification of nested filtered inputs, partial scope
        // opacity and structural/reveal membership. Reject these profiles before
        // ordinary children can accidentally stand in for the composed result.
        if source.parent_scope().is_some()
            || row.style.opacity != 1.0
            || row.appearance != 1.0
            || row.spatial.is_some()
            || row.draw_enabled
            || !publication.frame().is_present(index)
        {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        if self
            .scopes
            .get(&index)
            .is_some_and(|old| old.owner == row.id && Arc::ptr_eq(&old.source, source))
        {
            return Ok(());
        }
        let mut members = Vec::with_capacity(source.members().len());
        let mut previous = None;
        for &object in source.members() {
            let member = publication
                .object_index(object)
                .ok_or(EffectPrepareError::PublicationMismatch)?;
            self.validate_member(publication, member)?;
            let rank = ranks[member];
            if rank == usize::MAX || previous.is_some_and(|previous| rank != previous + 1) {
                return Err(EffectPrepareError::PublicationMismatch);
            }
            if self
                .dependents
                .get(&member)
                .is_some_and(|(owner, _)| *owner != index)
            {
                return Err(EffectPrepareError::UnsupportedCapture);
            }
            previous = Some(rank);
            members.push(member);
        }
        self.remove(index);
        for (&object, &member) in source.members().iter().zip(&members) {
            self.dependents.insert(member, (index, object));
            self.placements.insert(object, index);
        }
        self.scopes.insert(
            index,
            ComposedCapture {
                owner: row.id,
                source: source.clone(),
                members,
            },
        );
        Ok(())
    }

    fn remove(&mut self, index: usize) {
        if let Some(old) = self.scopes.remove(&index) {
            for (&object, member) in old.source.members().iter().zip(old.members) {
                self.dependents.remove(&member);
                self.placements.remove(&object);
            }
        }
    }

    fn validate_member(
        &self,
        publication: &RendererPublication<'_>,
        index: usize,
    ) -> Result<(), EffectPrepareError> {
        let row = publication
            .frame()
            .objects
            .get(index)
            .ok_or(EffectPrepareError::PublicationMismatch)?;
        if !publication.frame().is_drawable(index)
            || row.spatial.is_some()
            || row.effects.is_some()
            || row.content.composed_source().is_some()
        {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        Ok(())
    }

    pub(super) fn changed_member(
        &self,
        publication: &RendererPublication<'_>,
        index: usize,
    ) -> Result<Option<usize>, EffectPrepareError> {
        let Some(&(scope, expected)) = self.dependents.get(&index) else {
            return Ok(None);
        };
        if publication
            .frame()
            .objects
            .get(scope)
            .is_some_and(|row| row.effects.is_none())
        {
            return Ok(None);
        }
        self.validate_member(publication, index)?;
        if publication.frame().objects[index].id != expected {
            return Err(EffectPrepareError::PublicationMismatch);
        }
        Ok(Some(scope))
    }

    pub(super) fn visibility<'a>(
        &'a self,
        active: &'a BTreeMap<usize, ObjectId>,
    ) -> impl Iterator<Item = usize> + 'a {
        self.dependents
            .iter()
            .filter_map(|(&member, &(scope, _))| active.contains_key(&scope).then_some(member))
    }

    pub(super) fn placement(&self, object: ObjectId) -> Option<(ObjectId, bool)> {
        let scope = self.scopes.get(self.placements.get(&object)?)?;
        Some((scope.owner, scope.source.members().first() == Some(&object)))
    }

    pub(super) fn dependent(&self, index: usize) -> Option<usize> {
        self.dependents.get(&index).map(|&(scope, _)| scope)
    }

    pub(super) fn validate_order(
        &self,
        ranks: &[usize],
        index: usize,
    ) -> Result<(), EffectPrepareError> {
        let Some(scope) = self.scopes.get(&index) else {
            return Ok(());
        };
        if scope
            .members
            .windows(2)
            .any(|pair| ranks[pair[0]].checked_add(1) != Some(ranks[pair[1]]))
        {
            return Err(EffectPrepareError::PublicationMismatch);
        }
        Ok(())
    }
}
