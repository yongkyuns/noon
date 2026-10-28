//! Borrow unchanged rows and stage only mutation metadata and shifted ranks.
use super::*;
use noon_core::{ObjectContentRef, Transform2D};
use std::sync::Arc;
mod order;
use order::ProjectedOrder;

#[derive(Clone)]
struct Row<'a> {
    source: Option<&'a Arc<ComposedSource>>,
    transform: Transform2D,
    spatial: Option<Box<crate::CompiledSpatialState>>,
    draw: bool,
    painted: bool,
    text_bounds: bool,
    z: f64,
    input_role: EffectInputRole,
}
impl<'a> Row<'a> {
    fn from_object(row: &'a crate::CompiledObject) -> Self {
        Self {
            source: if let ObjectContentRef::Composed(source) = &row.content {
                Some(source)
            } else {
                None
            },
            transform: row.base_transform,
            spatial: row.spatial.clone(),
            draw: row.draw_enabled,
            painted: row.base_style.fill.is_some()
                || row.base_style.stroke.is_some()
                || row.fill_paint.is_some(),
            text_bounds: row.text_bounds.is_some(),
            z: row.base_z_index,
            input_role: row
                .effects
                .as_ref()
                .map_or(EffectInputRole::Painted, |stack| stack.input_role()),
        }
    }
}
pub(super) struct Projection<'a> {
    scene: &'a CompiledScene,
    rows: BTreeMap<ObjectId, Option<Row<'a>>>,
    changed: BTreeSet<ObjectId>,
    replaced: BTreeSet<ObjectId>,
    dependencies: ComposedDependencyChanges<'a>,
    family: ProjectedOrder<'a>,
    painter: ProjectedOrder<'a>,
}
impl<'a> Projection<'a> {
    pub(super) fn new(scene: &'a CompiledScene) -> Self {
        Self::with_index(scene, &scene.composed_dependencies)
    }
    pub(super) fn with_index(scene: &'a CompiledScene, base: &'a ComposedDependencies) -> Self {
        Self {
            scene,
            rows: BTreeMap::new(),
            changed: BTreeSet::new(),
            replaced: BTreeSet::new(),
            dependencies: ComposedDependencyChanges::new(base),
            family: ProjectedOrder::new(scene, true),
            painter: ProjectedOrder::new(scene, false),
        }
    }
    fn row(&self, id: ObjectId) -> Option<Row<'a>> {
        if let Some(row) = self.rows.get(&id) {
            return row.clone();
        }
        self.scene
            .object_index(id)
            .map(|index| Row::from_object(&self.scene.objects[index as usize]))
    }
    fn edit(&mut self, id: ObjectId, edit: impl FnOnce(&mut Row<'a>)) {
        if let Some(mut row) = self.row(id) {
            edit(&mut row);
            self.rows.insert(id, Some(row));
        }
    }
    pub(super) fn source(&self, id: ObjectId) -> Option<&'a ComposedSource> {
        self.row(id)?.source.map(|source| source.as_ref())
    }
    pub(super) fn owner_valid(&self, id: ObjectId) -> bool {
        self.row(id).is_some_and(|row| {
            row.source.is_some()
                && row.transform == Transform2D::IDENTITY
                && row.spatial.is_none()
                && !row.draw
                && !row.painted
                && !row.text_bounds
        })
    }
    pub(super) fn input_valid(&self, id: ObjectId) -> Option<bool> {
        self.row(id)
            .map(|row| row.source.is_none() && row.spatial.is_none())
    }
    pub(super) fn rank(&self, id: ObjectId) -> Option<usize> {
        self.painter.rank(id)
    }
    pub(super) fn painter_rows_staged(&self) -> usize {
        self.painter.staged_len()
    }
    fn reposition(&mut self, id: ObjectId) {
        // The order mutator needs only a shared view of staged property keys.
        let rows = &self.rows;
        let scene = self.scene;
        let family = &self.family;
        self.painter.reposition(id, |a, b| {
            let key = |id| {
                rows.get(&id)
                    .and_then(Option::as_ref)
                    .map(|row| (row.source.is_some(), row.z))
                    .or_else(|| {
                        scene.object_index(id).map(|index| {
                            let row = &scene.objects[index as usize];
                            (row.content.composed_source().is_some(), row.base_z_index)
                        })
                    })
                    .expect("existing projected painter row")
            };
            let (ak, az) = key(a);
            let (bk, bz) = key(b);
            ak.cmp(&bk)
                .then_with(|| az.total_cmp(&bz))
                .then_with(|| family.rank(a).cmp(&family.rank(b)))
        });
    }
    fn stage_input_role(&mut self, id: ObjectId, input_role: EffectInputRole) {
        if self.row(id).is_some_and(|row| row.input_role != input_role) {
            self.edit(id, |row| row.input_role = input_role);
            self.changed.insert(id);
        }
    }
    pub(super) fn stage(
        &mut self,
        patches: &'a [ExecutionPatch],
    ) -> Result<(), CompiledComposedSourceError> {
        for patch in patches {
            match patch {
                ExecutionPatch::CreateObject(row) => {
                    if self.row(row.id).is_some() {
                        continue;
                    } // ordinary preflight reports duplicate identity
                    self.rows.insert(row.id, Some(Row::from_object(row)));
                    self.changed.insert(row.id);
                    self.replaced.insert(row.id);
                    self.family.append(row.id);
                    self.painter.append(row.id);
                    self.reposition(row.id);
                }
                ExecutionPatch::RemoveObject(id) => {
                    self.family.remove(*id);
                    self.painter.remove(*id);
                    self.rows.insert(*id, None);
                    self.changed.insert(*id);
                    self.replaced.insert(*id);
                }
                ExecutionPatch::SetContent {
                    object,
                    content,
                    text_bounds,
                } => {
                    let source = if let ObjectContentRef::Composed(source) = content {
                        Some(source)
                    } else {
                        None
                    };
                    if self
                        .row(*object)
                        .is_some_and(|old| old.source.is_some() != source.is_some())
                    {
                        return Err(CompiledComposedSourceError {
                            owner: *object,
                            kind: CompiledComposedSourceErrorKind::ScopeRoleChanged,
                        });
                    }
                    let replaces_plan = self
                        .row(*object)
                        .and_then(|row| row.source)
                        .zip(source)
                        .is_some_and(|(old, new)| !Arc::ptr_eq(old, new));
                    self.edit(*object, |row| {
                        row.source = source;
                        row.text_bounds = text_bounds.is_some();
                    });
                    if replaces_plan {
                        self.changed.insert(*object);
                        self.replaced.insert(*object);
                    }
                }
                ExecutionPatch::SetEffects { object, effects } => {
                    self.stage_input_role(*object, effects.input_role());
                }
                ExecutionPatch::SetEffectStack {
                    object, effects, ..
                } => {
                    self.stage_input_role(
                        *object,
                        effects
                            .as_ref()
                            .map_or(EffectInputRole::Painted, |stack| stack.input_role()),
                    );
                }
                ExecutionPatch::SetZIndex { object, value } => {
                    self.edit(*object, |row| row.z = *value);
                    self.reposition(*object);
                }
                ExecutionPatch::ReorderObject { object, before } => {
                    let Some(position) = self.family.rank(*object) else {
                        continue;
                    };
                    if before == &Some(*object) {
                        continue;
                    }
                    let destination = match before {
                        Some(before) => {
                            let Some(rank) = self.family.rank(*before) else {
                                continue;
                            };
                            rank - usize::from(position < rank)
                        }
                        None => self.family.tail(),
                    };
                    self.family.move_to(*object, destination);
                    self.reposition(*object);
                }
                ExecutionPatch::SetSpatialState {
                    object,
                    base_transform,
                    spatial,
                } => {
                    let domain_changed = self
                        .row(*object)
                        .is_some_and(|row| row.spatial.is_some() != spatial.is_some());
                    self.edit(*object, |row| {
                        row.transform = *base_transform;
                        row.spatial = spatial.clone().map(Box::new);
                    });
                    if domain_changed {
                        self.changed.insert(*object);
                    }
                }
                ExecutionPatch::SetSemanticTransform { object, transform } => {
                    if let Some(row) = self.row(*object) {
                        if let Some((base, spatial)) = crate::lower_semantic_transform_patch(
                            *object,
                            row.spatial.as_deref(),
                            *transform,
                        ) {
                            let domain_changed = row.spatial.is_some() != spatial.is_some();
                            self.edit(*object, |row| {
                                row.transform = base;
                                row.spatial = spatial.map(Box::new);
                            });
                            if domain_changed {
                                self.changed.insert(*object);
                            }
                        }
                    }
                }
                ExecutionPatch::SetTransform { object, transform }
                    if self.source(*object).is_some() =>
                {
                    self.edit(*object, |row| row.transform = *transform);
                }
                ExecutionPatch::SetStyle {
                    object,
                    style,
                    fill_paint,
                } if self.source(*object).is_some() => {
                    self.edit(*object, |row| {
                        row.painted =
                            style.fill.is_some() || style.stroke.is_some() || fill_paint.is_some()
                    });
                }
                ExecutionPatch::SetDrawEnabled { object, enabled }
                    if self.source(*object).is_some() =>
                {
                    self.edit(*object, |row| row.draw = *enabled);
                }
                _ => {}
            }
        }
        for &id in self.rows.keys() {
            if self.source(id).is_some() && !self.owner_valid(id) {
                return Err(CompiledComposedSourceError {
                    owner: id,
                    kind: CompiledComposedSourceErrorKind::InvalidOwner,
                });
            }
        }
        for &id in &self.replaced {
            self.dependencies.replace(id, None, self.source(id));
        }
        Ok(())
    }
    pub(super) fn affected_scopes(&self) -> BTreeSet<ObjectId> {
        self.dependencies.affected_scopes(
            self,
            self.changed.iter().copied(),
            self.painter.touched(),
        )
    }
}

impl ComposedSourceGraphView for Projection<'_> {
    fn source(&self, owner: ObjectId) -> Option<&ComposedSource> {
        self.source(owner)
    }
    fn owner_valid(&self, owner: ObjectId) -> bool {
        self.owner_valid(owner)
    }
    fn input_valid(&self, input: ObjectId) -> Option<bool> {
        self.input_valid(input)
    }
    fn rank(&self, input: ObjectId) -> Option<usize> {
        self.rank(input)
    }
    fn input_role(&self, owner: ObjectId) -> Option<EffectInputRole> {
        self.row(owner).map(|row| row.input_role)
    }
    fn overlapping(&self, input: ObjectId) -> impl Iterator<Item = ObjectId> {
        self.dependencies.overlapping(input)
    }
}

pub(super) fn introduces_scope(patch: &ExecutionPatch) -> bool {
    match patch {
        ExecutionPatch::CreateObject(row) => {
            row.content.composed_source().is_some()
                || row
                    .effects
                    .as_ref()
                    .is_some_and(|stack| stack.input_role() == EffectInputRole::PainterPrefix)
        }
        ExecutionPatch::SetContent { content, .. } => content.composed_source().is_some(),
        ExecutionPatch::SetEffects { effects, .. } => {
            effects.input_role() == EffectInputRole::PainterPrefix
        }
        ExecutionPatch::SetEffectStack { effects, .. } => effects
            .as_ref()
            .is_some_and(|stack| stack.input_role() == EffectInputRole::PainterPrefix),
        _ => false,
    }
}
pub(super) fn affects_source_graph(patch: &ExecutionPatch) -> bool {
    matches!(
        patch,
        ExecutionPatch::CreateObject(_)
            | ExecutionPatch::RemoveObject(_)
            | ExecutionPatch::ReorderObject { .. }
            | ExecutionPatch::SetZIndex { .. }
            | ExecutionPatch::SetContent { .. }
            | ExecutionPatch::SetEffects { .. }
            | ExecutionPatch::SetEffectStack { .. }
            | ExecutionPatch::SetSpatialState { .. }
            | ExecutionPatch::SetSemanticTransform { .. }
            | ExecutionPatch::SetTransform { .. }
            | ExecutionPatch::SetStyle { .. }
            | ExecutionPatch::SetDrawEnabled { .. }
    )
}
