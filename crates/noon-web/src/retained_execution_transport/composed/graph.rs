//! Borrow installed rows and sparse final overlays into the compiler's validator.
use super::*;
use noon_compile::{
    CompiledComposedSourceError, CompiledComposedSourceErrorKind as Kind, ComposedDependencies,
    ComposedDependencyChanges, ComposedSourceGraphView,
};
use std::collections::{BTreeSet, HashMap};

pub(in crate::retained_execution_transport) struct ResolvedRow<'a> {
    pub index: usize,
    pub added: bool,
    pub wire: &'a RetainedTransportObjectState,
    pub geometry: Option<Arc<GeometryRef>>,
    pub state: FrameObjectState,
}

#[derive(Clone, Copy)]
struct Row<'a> {
    state: &'a FrameObjectState,
    present: bool,
}

struct View<'a> {
    mirror: &'a RetainedExecutionFrameMirror,
    rows: HashMap<ObjectId, Option<Row<'a>>>,
    ranks: HashMap<ObjectId, Option<usize>>,
    dependencies: ComposedDependencyChanges<'a>,
    snapshot: bool,
}
impl<'a> View<'a> {
    fn row(&self, id: ObjectId) -> Option<Row<'a>> {
        if let Some(row) = self.rows.get(&id) {
            return *row;
        }
        if self.snapshot {
            return None;
        }
        let &index = self.mirror.object_indices.get(&id)?;
        self.mirror.painter_ranks.get(index).copied().flatten()?;
        let frame = self.mirror.frame.as_ref()?;
        Some(Row {
            state: &frame.objects[index],
            present: frame.presences[index],
        })
    }
    fn validate(
        &self,
        affected: &BTreeSet<ObjectId>,
    ) -> Result<noon_compile::ComposedValidationStats, RetainedExecutionTransportError> {
        let stats = noon_compile::validate_composed_sources(self, affected)
            .map_err(RetainedExecutionTransportError::InvalidComposedSource)?;
        let mut checked = HashSet::new();
        for &owner in affected {
            let Some(row) = self.row(owner) else {
                continue;
            };
            if row.state.effects.is_none() && row.state.style.opacity == 1.0 {
                continue;
            }
            let Some(source) = self.source(owner) else {
                continue;
            };
            for &input in source.members() {
                if checked.insert(input)
                    && self
                        .row(input)
                        .is_none_or(|row| !row.present || !row.state.draw_enabled)
                {
                    return Err(graph_error(owner, Kind::NonDrawableInput(input)));
                }
            }
        }
        Ok(stats)
    }
}
impl ComposedSourceGraphView for View<'_> {
    fn source(&self, owner: ObjectId) -> Option<&ComposedSource> {
        self.row(owner)?.state.content.composed_source()
    }
    fn owner_valid(&self, owner: ObjectId) -> bool {
        self.row(owner).is_some_and(|row| {
            !row.state.draw_enabled
                && row.state.spatial.is_none()
                && row.state.transform == Transform2D::IDENTITY
                && row.state.style.fill.is_none()
                && row.state.style.stroke.is_none()
                && row.state.fill_paint.is_none()
                && row.state.text_bounds.is_none()
        })
    }
    fn input_valid(&self, input: ObjectId) -> Option<bool> {
        self.row(input)
            .map(|row| row.state.content.composed_source().is_none() && row.state.spatial.is_none())
    }
    fn rank(&self, input: ObjectId) -> Option<usize> {
        if let Some(rank) = self.ranks.get(&input) {
            return *rank;
        }
        if self.snapshot {
            return None;
        }
        let &index = self.mirror.object_indices.get(&input)?;
        self.mirror
            .painter_ranks
            .get(index)
            .copied()
            .flatten()
            .map(|rank| rank as usize)
    }
    fn overlapping(&self, input: ObjectId) -> impl Iterator<Item = ObjectId> {
        self.dependencies.overlapping(input)
    }
}
fn graph_error(owner: ObjectId, kind: Kind) -> RetainedExecutionTransportError {
    RetainedExecutionTransportError::InvalidComposedSource(CompiledComposedSourceError {
        owner,
        kind,
    })
}
fn source_arc(content: &ObjectContentRef) -> Option<&Arc<ComposedSource>> {
    if let ObjectContentRef::Composed(source) = content {
        Some(source)
    } else {
        None
    }
}

#[derive(Default)]
pub(in crate::retained_execution_transport) struct PreparedComposedChanges {
    replacements: Vec<SourceReplacement>,
    #[cfg(test)]
    pub stats: noon_compile::ComposedValidationStats,
}
struct SourceReplacement {
    owner: ObjectId,
    previous: Option<Arc<ComposedSource>>,
    next: Option<Arc<ComposedSource>>,
}
impl PreparedComposedChanges {
    pub(in crate::retained_execution_transport) fn commit(self, index: &mut ComposedDependencies) {
        for SourceReplacement {
            owner,
            previous,
            next,
        } in self.replacements
        {
            index.replace(owner, previous.as_deref(), next.as_deref());
        }
    }
}
impl RetainedExecutionFrameMirror {
    pub(in crate::retained_execution_transport) fn preflight_composed_snapshot(
        &self,
        rows: &[FrameObjectState],
        wire: &[RetainedTransportObjectState],
    ) -> Result<ComposedDependencies, RetainedExecutionTransportError> {
        let mut index = ComposedDependencies::default();
        for row in rows {
            index.replace(row.id, None, row.content.composed_source());
        }
        if index.is_empty() {
            return Ok(index);
        }
        let view = View {
            mirror: self,
            snapshot: true,
            rows: rows
                .iter()
                .zip(wire)
                .map(|(row, wire)| {
                    (
                        row.id,
                        Some(Row {
                            state: row,
                            present: wire.presence,
                        }),
                    )
                })
                .collect(),
            ranks: rows
                .iter()
                .enumerate()
                .map(|(rank, row)| (row.id, Some(rank)))
                .collect(),
            dependencies: ComposedDependencyChanges::new(&index),
        };
        view.validate(&index.owners().collect())?;
        Ok(index)
    }
    pub(in crate::retained_execution_transport) fn preflight_composed_incremental(
        &self,
        rows: &[ResolvedRow<'_>],
        removed: &[usize],
        order: Option<&PreparedPainterOrder>,
    ) -> Result<PreparedComposedChanges, RetainedExecutionTransportError> {
        if self.composed_dependencies.is_empty()
            && !rows
                .iter()
                .any(|row| row.state.content.composed_source().is_some())
        {
            return Ok(PreparedComposedChanges::default());
        }
        let frame = self.frame.as_ref().expect("installed incremental frame");
        let mut view = View {
            mirror: self,
            snapshot: false,
            rows: HashMap::new(),
            ranks: HashMap::new(),
            dependencies: ComposedDependencyChanges::new(&self.composed_dependencies),
        };
        let mut prepared = PreparedComposedChanges::default();
        let mut changed = BTreeSet::new();
        for &index in removed {
            let old = &frame.objects[index];
            if let Some(source) = source_arc(&old.content) {
                view.dependencies.replace(old.id, Some(source), None);
                prepared.replacements.push(SourceReplacement {
                    owner: old.id,
                    previous: Some(source.clone()),
                    next: None,
                });
            }
            view.rows.insert(old.id, None);
            view.ranks.insert(old.id, None);
            changed.insert(old.id);
        }
        for row in rows {
            let old = (!row.added
                && self
                    .painter_ranks
                    .get(row.index)
                    .copied()
                    .flatten()
                    .is_some())
            .then(|| &frame.objects[row.index]);
            let previous = old.and_then(|old| source_arc(&old.content));
            let next = source_arc(&row.state.content);
            if old.is_some() && previous.is_some() != next.is_some() {
                return Err(graph_error(row.state.id, Kind::ScopeRoleChanged));
            }
            let same = match (previous, next) {
                (Some(old), Some(new)) => Arc::ptr_eq(old, new),
                (None, None) => true,
                _ => false,
            };
            if !same {
                view.dependencies.replace(
                    row.state.id,
                    previous.map(AsRef::as_ref),
                    next.map(AsRef::as_ref),
                );
                prepared.replacements.push(SourceReplacement {
                    owner: row.state.id,
                    previous: previous.cloned(),
                    next: next.cloned(),
                });
            }
            if old.is_none_or(|old| {
                old.spatial.is_some() != row.state.spatial.is_some()
                    || old.draw_enabled != row.state.draw_enabled
                    || frame.presences[row.index] != row.wire.presence
                    || old.effects.is_some() != row.state.effects.is_some()
                    || (old.style.opacity != 1.0) != (row.state.style.opacity != 1.0)
            }) {
                changed.insert(row.state.id);
            }
            view.rows.insert(
                row.state.id,
                Some(Row {
                    state: &row.state,
                    present: row.wire.presence,
                }),
            );
        }
        let mut painter_rows = Vec::new();
        if let Some(order) = order {
            let updated_ids = rows
                .iter()
                .map(|row| (row.index, row.state.id))
                .collect::<HashMap<_, _>>();
            let id_for_index = |index: usize| {
                updated_ids
                    .get(&index)
                    .copied()
                    .unwrap_or_else(|| frame.objects[index].id)
            };
            for &index in &self.painter_order[order.range.start..order.old_end] {
                let id = frame.objects[index as usize].id;
                view.ranks.insert(id, None);
                painter_rows.push(id);
            }
            for (offset, &index) in order.segment.iter().enumerate() {
                let id = id_for_index(index as usize);
                view.ranks.insert(id, Some(order.range.start + offset));
                painter_rows.push(id);
            }
        }
        let affected = view
            .dependencies
            .affected_scopes(&view, changed, painter_rows);
        let _stats = view.validate(&affected)?;
        #[cfg(test)]
        {
            prepared.stats = _stats;
        }
        Ok(prepared)
    }
}
