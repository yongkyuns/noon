//! Disposable input/parent indices for composed capture and painter placement.
//! Membership plans remain immutable execution data; parameter values are borrowed.
use super::*;

#[derive(Debug)]
pub(super) struct ComposedCapture {
    pub(super) owner: ObjectId,
    pub(super) source: Arc<noon_core::ComposedSource>,
    members: Vec<usize>,
    parent: Option<usize>,
    pub(super) depth: Option<usize>,
}

#[derive(Debug, Default)]
pub(super) struct ComposedCaptures {
    pub(super) scopes: BTreeMap<usize, ComposedCapture>,
    dependents: BTreeMap<usize, (ObjectId, BTreeSet<usize>)>,
    placements: BTreeMap<ObjectId, BTreeSet<usize>>,
    children: BTreeMap<usize, BTreeSet<usize>>,
    topology_dirty: BTreeSet<usize>,
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
        if publication.object_index(row.id) != Some(index) {
            self.remove(index);
            return Ok(());
        }
        // #1897: composite opacity and presence-changing membership are still
        // unqualified. Nested inputs themselves share this exact capture path.
        // Without attachments this row records containment only; capture-only
        // eligibility must not change ordinary drawing after effect removal.
        if row.transform != Transform2D::IDENTITY
            || row.spatial.is_some()
            || row.draw_enabled
            || (row.effects.is_some()
                && (row.style.opacity != 1.0
                    || row.appearance != 1.0
                    || !publication.frame().is_present(index)))
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
        let parent = source
            .parent_scope()
            .map(|id| {
                let parent = publication
                    .object_index(id)
                    .ok_or(EffectPrepareError::PublicationMismatch)?;
                if publication.frame().objects[parent]
                    .content
                    .composed_source()
                    .is_none()
                {
                    return Err(EffectPrepareError::PublicationMismatch);
                }
                Ok(parent)
            })
            .transpose()?;
        let mut members = Vec::with_capacity(source.members().len());
        let mut previous = None;
        for &object in source.members() {
            let member = publication
                .object_index(object)
                .ok_or(EffectPrepareError::PublicationMismatch)?;
            self.validate_member(publication, member, row.effects.is_some())?;
            let rank = ranks[member];
            if rank == usize::MAX
                || previous.is_some_and(|previous: usize| previous.checked_add(1) != Some(rank))
            {
                return Err(EffectPrepareError::PublicationMismatch);
            }
            previous = Some(rank);
            members.push(member);
        }
        let old_parent = self.scopes.get(&index).and_then(|old| old.parent);
        let old_depth = self.scopes.get(&index).and_then(|old| old.depth);
        self.remove(index);
        for (&object, &member) in source.members().iter().zip(&members) {
            let entry = self
                .dependents
                .entry(member)
                .or_insert_with(|| (object, BTreeSet::new()));
            if entry.0 != object {
                return Err(EffectPrepareError::PublicationMismatch);
            }
            entry.1.insert(index);
            self.placements.entry(object).or_default().insert(index);
        }
        if let Some(parent) = parent {
            self.children.entry(parent).or_default().insert(index);
        }
        let depth = if old_parent == parent {
            old_depth
        } else {
            None
        };
        self.scopes.insert(
            index,
            ComposedCapture {
                owner: row.id,
                source: source.clone(),
                members,
                parent,
                depth,
            },
        );
        self.topology_dirty.insert(index);
        Ok(())
    }

    fn remove(&mut self, index: usize) {
        if let Some(old) = self.scopes.remove(&index) {
            for (&object, member) in old.source.members().iter().zip(old.members) {
                if let Some((_, scopes)) = self.dependents.get_mut(&member) {
                    scopes.remove(&index);
                    if scopes.is_empty() {
                        self.dependents.remove(&member);
                    }
                }
                if let Some(scopes) = self.placements.get_mut(&object) {
                    scopes.remove(&index);
                    if scopes.is_empty() {
                        self.placements.remove(&object);
                    }
                }
            }
            if let Some(parent) = old.parent {
                if let Some(children) = self.children.get_mut(&parent) {
                    children.remove(&index);
                    if children.is_empty() {
                        self.children.remove(&parent);
                    }
                }
            }
            self.topology_dirty.insert(index);
        }
    }

    pub(super) fn finish_topology(&mut self) -> Result<(), EffectPrepareError> {
        let mut dirty = std::mem::take(&mut self.topology_dirty);
        let mut pending = dirty.iter().copied().collect::<Vec<_>>();
        while let Some(index) = pending.pop() {
            for &child in self.children.get(&index).into_iter().flatten() {
                if dirty.insert(child) {
                    pending.push(child);
                }
            }
        }
        for &index in &dirty {
            if let Some(scope) = self.scopes.get_mut(&index) {
                scope.depth = None;
            }
        }
        for index in dirty {
            let mut current = index;
            let mut path = Vec::new();
            let mut visited = BTreeSet::new();
            let mut next_depth = 0;
            loop {
                let Some(scope) = self.scopes.get(&current) else {
                    if path.is_empty() {
                        break;
                    }
                    return Err(EffectPrepareError::PublicationMismatch);
                };
                if let Some(depth) = scope.depth {
                    next_depth = depth + 1;
                    break;
                }
                if !visited.insert(current) {
                    return Err(EffectPrepareError::PublicationMismatch);
                }
                path.push(current);
                if let Some(parent) = scope.parent {
                    current = parent;
                } else {
                    break;
                }
            }
            if path.is_empty() {
                continue;
            }
            for index in path.into_iter().rev() {
                self.scopes
                    .get_mut(&index)
                    .expect("existing capture ancestor")
                    .depth = Some(next_depth);
                next_depth += 1;
            }
        }
        Ok(())
    }

    fn validate_member(
        &self,
        publication: &RendererPublication<'_>,
        index: usize,
        require_drawable: bool,
    ) -> Result<(), EffectPrepareError> {
        let row = publication
            .frame()
            .objects
            .get(index)
            .ok_or(EffectPrepareError::PublicationMismatch)?;
        if (require_drawable && !publication.frame().is_drawable(index))
            || row.spatial.is_some()
            || row.content.composed_source().is_some()
        {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        Ok(())
    }

    pub(super) fn ancestors(&self, index: usize) -> impl Iterator<Item = usize> + '_ {
        let mut parent = self.scopes.get(&index).and_then(|scope| scope.parent);
        let mut remaining = self.scopes.len();
        std::iter::from_fn(move || {
            if remaining == 0 {
                return None;
            }
            remaining -= 1;
            let current = parent?;
            parent = self.scopes.get(&current).and_then(|scope| scope.parent);
            Some(current)
        })
    }

    pub(super) fn visibility<'a>(
        &'a self,
        active: &'a BTreeMap<usize, ObjectId>,
    ) -> impl Iterator<Item = usize> + 'a {
        self.dependents.iter().filter_map(|(&member, (_, scopes))| {
            scopes
                .iter()
                .any(|scope| active.contains_key(scope))
                .then_some(member)
        })
    }

    pub(super) fn placement(
        &self,
        object: ObjectId,
        enabled: impl Fn(ObjectId) -> bool,
    ) -> Option<(ObjectId, bool)> {
        let scope = self
            .placements
            .get(&object)?
            .iter()
            .filter_map(|index| self.scopes.get(index))
            .filter(|scope| enabled(scope.owner))
            .min_by_key(|scope| scope.depth)?;
        Some((scope.owner, scope.source.members().first() == Some(&object)))
    }

    pub(super) fn dependents(&self, index: usize) -> impl Iterator<Item = usize> + '_ {
        self.dependents
            .get(&index)
            .into_iter()
            .flat_map(|(_, scopes)| scopes.iter().copied())
    }
    pub(super) fn depth(&self, index: usize) -> Option<usize> {
        self.scopes.get(&index).and_then(|scope| scope.depth)
    }
    pub(super) fn candidates(
        &self,
        owner: usize,
        member: usize,
    ) -> impl Iterator<Item = (usize, &ComposedCapture)> + '_ {
        let depth = self.depth(owner);
        self.dependents(member).filter_map(move |index| {
            let scope = self.scopes.get(&index)?;
            (scope.depth > depth).then_some((index, scope))
        })
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

    pub(super) fn validate_inputs(
        &self,
        publication: &RendererPublication<'_>,
        index: usize,
    ) -> Result<(), EffectPrepareError> {
        let Some(scope) = self.scopes.get(&index) else {
            return Ok(());
        };
        let require_drawable = publication.frame().objects[index].effects.is_some();
        for (&object, &member) in scope.source.members().iter().zip(&scope.members) {
            if publication.object_index(object) != Some(member) {
                return Err(EffectPrepareError::PublicationMismatch);
            }
            self.validate_member(publication, member, require_drawable)?;
        }
        Ok(())
    }
}
