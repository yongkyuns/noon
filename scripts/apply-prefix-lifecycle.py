#!/usr/bin/env python3
"""One-use exact-source integration for #2211; removed after qualification."""
from pathlib import Path


def replace(path, old, new):
    p = Path(path)
    text = p.read_text()
    if text.count(old) != 1:
        raise RuntimeError(f'Unexpected source anchor in {path}: {old!r}')
    p.write_text(text.replace(old, new, 1))


def write(path, content):
    p = Path(path)
    if p.exists():
        raise RuntimeError(f'Refusing to overwrite {path}')
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(content)


p = Path('crates/noon-compile/src/replay_revision/prefix.rs')
s = p.read_text()
s = s.replace('//! Experimental fixed-membership prefix transfer.', '//! Experimental sparse lifecycle/content prefix transfer.')
s = s.replace('use std::collections::BTreeMap;', 'use std::collections::BTreeMap;\n\nmod order;\nmod resources;\nmod storage;\nuse order::SparseOrder;')
s = s.replace('Property::Transform\n', 'Property::Presence\n            | Property::ZIndex\n            | Property::Transform\n', 1)
s = s.replace('''    /// Admission is deliberately narrow: existing live rows, unchanged content,
    /// order, glow and spatial domains, and ordinary property tracks. Structural,
    /// family, path-motion and external-resource extension need separate support.''', '''    /// Ordinary 2D lifecycle, content and property tracks are admitted. Family,
    /// path-motion, spatial and glow plans still require separate qualification.
    /// Ordering reconstruction visits only affected rows and their neighbours;
    /// it never clones/scans the full painter/family order.''')
s = s.replace('        let mut row_overlay = BTreeMap::new();', '        let mut row_overlay = BTreeMap::new();\n        let mut order = SparseOrder::new(self);')
a = s.index('            let mut rows = Vec::with_capacity(old.rows.len());')
b = s.index('            let mut tracks = Vec::with_capacity(old.tracks.len());', a)
s = s[:a] + '''            // Current compiler structural patches each own a single order row.
            // Do not silently invent atomic multi-row exchange semantics.
            if old.rows.iter().filter(|row| row.order.is_some()).count() > 1 {
                return None;
            }
            let mut rows = Vec::with_capacity(old.rows.len());
            for saved in &old.rows {
                let physical = self.objects.get(saved.index as usize)?;
                let current = row_overlay.entry(saved.index).or_insert_with(|| {
                    physical.live.then(|| physical.clone())
                });
                for object in saved.object.iter().chain(current.iter()) {
                    if !object.live || object.id != physical.id
                        || object.spatial.is_some() || object.glow.is_some() {
                        return None;
                    }
                }
                // Capture the actual opposite membership/order, then undo this
                // one local order change in the sparse scratch view.
                let forward_order = if let Some(before) = saved.order {
                    let after = order.successor(saved.index);
                    order.restore(saved.index, saved.object.is_some(), before)?;
                    Some(after)
                } else {
                    if saved.object.is_none() || current.is_none() { return None; }
                    None
                };
                rows.push(SavedRow {
                    index: saved.index, object: current.clone(), order: forward_order,
                });
                *current = saved.object.clone();
            }
''' + s[b:]
p.write_text(s)

write('crates/noon-compile/src/replay_revision/prefix/order.rs', '''//! Sparse reverse ordering overlay; it owns no execution or semantic state.
use super::CompiledScene;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct Links { previous: Option<u32>, next: Option<u32> }

pub(super) struct SparseOrder<'a> {
    scene: &'a CompiledScene,
    changed: BTreeMap<u32, Option<Links>>,
    tail: Option<u32>,
}
impl<'a> SparseOrder<'a> {
    pub(super) fn new(scene: &'a CompiledScene) -> Self {
        Self { scene, changed: BTreeMap::new(), tail: scene.family_order.last().copied() }
    }
    fn links(&mut self, index: u32) -> Option<Links> {
        *self.changed.entry(index).or_insert_with(|| {
            let rank = self.scene.family_rank(index)? as usize;
            Some(Links {
                previous: rank.checked_sub(1).and_then(|rank| self.scene.family_order.get(rank).copied()),
                next: self.scene.family_order.get(rank + 1).copied(),
            })
        })
    }
    pub(super) fn successor(&mut self, index: u32) -> Option<u32> {
        self.links(index).and_then(|links| links.next)
    }
    pub(super) fn restore(&mut self, index: u32, live: bool, before: Option<u32>) -> Option<()> {
        if let Some(old) = self.links(index) {
            if let Some(previous) = old.previous {
                let mut links = self.links(previous)?;
                links.next = old.next;
                self.changed.insert(previous, Some(links));
            }
            if let Some(next) = old.next {
                let mut links = self.links(next)?;
                links.previous = old.previous;
                self.changed.insert(next, Some(links));
            } else { self.tail = old.previous; }
        }
        self.changed.insert(index, None);
        if live {
            // Match ordinary replay: an unavailable successor denotes the tail.
            let next = before.filter(|&anchor| self.links(anchor).is_some());
            let previous = match next {
                Some(next) => self.links(next)?.previous,
                None => self.tail,
            };
            if let Some(previous) = previous {
                let mut links = self.links(previous)?;
                links.next = Some(index);
                self.changed.insert(previous, Some(links));
            }
            if let Some(next) = next {
                let mut links = self.links(next)?;
                links.previous = Some(index);
                self.changed.insert(next, Some(links));
            } else { self.tail = Some(index); }
            self.changed.insert(index, Some(Links { previous, next }));
        }
        Some(())
    }
}
''')

write('crates/noon-compile/src/replay_revision/prefix/resources.rs', '''//! Existing immutable resource closures, filtered to not-yet-delivered handles.
use super::{CompiledReplayRevision, CompiledScene};
use crate::{CompiledResources, CompiledResourceError};
use noon_core::{FontResourceKey, GeometryRef, ObjectContentRef};

impl CompiledScene {
    /// Capture dependency closures for the new forward suffix only. Payloads
    /// remain shared Arcs; no pixel/font/path byte copies or full arena scans.
    pub fn prefix_resource_delta(
        &self, revisions: &[CompiledReplayRevision], known: &CompiledResources,
    ) -> Result<CompiledResources, CompiledResourceError> {
        let mut delta = CompiledResources::default();
        for revision in revisions {
            for row in &revision.rows {
                let Some(object) = &row.object else { continue; };
                match &object.content {
                    ObjectContentRef::Text(handle) => {
                        if known.texts.contains_key(handle) || delta.texts.contains_key(handle) { continue; }
                        let resource = self.resources.texts.get(handle)
                            .ok_or(CompiledResourceError::MissingText(*handle))?.clone();
                        delta.capture_text_payload(*handle, resource, |face| {
                            let key = FontResourceKey::from_face(face);
                            let handle = *self.resources.font_handles.get(&key)
                                .ok_or_else(|| CompiledResourceError::MissingFont(key.clone()))?;
                            let font = self.resources.fonts.get(&handle)
                                .ok_or(CompiledResourceError::MissingFont(key))?.clone();
                            Ok((handle, font))
                        }, |handle| {
                            self.resources.geometries.get(&handle).cloned()
                                .ok_or(CompiledResourceError::MissingGeometry(handle))
                        })?;
                    }
                    ObjectContentRef::Image(image) => {
                        let handle = image.resource();
                        if known.images.contains_key(&handle) || delta.images.contains_key(&handle) { continue; }
                        let resource = self.resources.images.get(&handle)
                            .ok_or(CompiledResourceError::MissingImage(handle))?.clone();
                        delta.images.insert(handle, resource);
                    }
                    ObjectContentRef::Geometry(GeometryRef::External(id)) => {
                        if known.geometry_handles.contains_key(id) || delta.geometry_handles.contains_key(id) { continue; }
                        let handle = *self.resources.geometry_handles.get(id)
                            .ok_or(CompiledResourceError::ConflictingGeometryId(*id))?;
                        let resource = self.resources.geometries.get(&handle)
                            .ok_or(CompiledResourceError::MissingGeometry(handle))?.clone();
                        delta.geometry_handles.insert(*id, handle);
                        delta.geometries.insert(handle, resource);
                    }
                    ObjectContentRef::Geometry(_) => {}
                }
            }
        }
        known.preflight_merge(&delta)?;
        delta.fonts.retain(|handle, _| !known.fonts.contains_key(handle));
        delta.font_handles.retain(|face, _| !known.font_handles.contains_key(face));
        delta.geometries.retain(|handle, _| !known.geometries.contains_key(handle));
        delta.geometry_handles.retain(|id, _| !known.geometry_handles.contains_key(id));
        Ok(delta)
    }
}
impl CompiledResources {
    /// Count immutable dependency entries, not bytes or process/VRAM usage.
    pub fn prefix_payload_count(&self) -> usize {
        self.images.len() + self.texts.len() + self.fonts.len() + self.geometries.len()
    }
    /// Record delivered handles using the existing resource projection type.
    pub fn extend_prefix_knowledge(&mut self, delta: &Self) {
        self.merge(delta.clone());
    }
}
''')

write('crates/noon-compile/src/replay_revision/prefix/storage.rs', '''//! Explicit storage maintenance for append-only historical object slots.
use super::CompiledScene;
use crate::CompiledObject;

impl CompiledScene {
    /// Named O(existing storage) capacity barrier. Never called implicitly from
    /// prefix append or ordinary local mutation. No logical state is changed.
    pub fn reserve_prefix_object_capacity(&mut self, total: usize) -> Result<(), std::collections::TryReserveError> {
        self.objects.try_reserve_exact(total.saturating_sub(self.objects.len()))?;
        self.family_ranks.try_reserve_exact(total.saturating_sub(self.family_ranks.len()))?;
        self.painter_ranks.try_reserve_exact(total.saturating_sub(self.painter_ranks.len()))?;
        self.family_order.try_reserve_exact(total.saturating_sub(self.family_order.len()))?;
        self.painter_order.try_reserve_exact(total.saturating_sub(self.painter_order.len()))?;
        Ok(())
    }
    /// Install source-derived tombstones, not visible membership. All identity,
    /// stream, dependency and capacity checks precede this infallible commit.
    pub fn install_prefix_tombstones(&mut self, objects: Vec<CompiledObject>) {
        for mut object in objects {
            let index = self.objects.len() as u32;
            object.live = false;
            object.dynamic = Default::default();
            self.retired_object_indices.insert(object.id, index);
            self.objects.push(object);
            self.family_ranks.push(None);
            self.painter_ranks.push(None);
        }
    }
}
''')

p = Path('crates/noon-runtime/src/replay/prefix.rs')
s = p.read_text().replace('use std::sync::Arc;', 'use std::sync::Arc;\nuse noon_compile::{CompiledObject, CompiledResources, CompiledResourceError};\n\nmod storage;')
s = s.replace('    Allocation,', '    Allocation,\n    CapacityRequired { object_slots: usize },\n    Resources(CompiledResourceError),')
s = s.replace('#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]', '''impl From<CompiledResourceError> for PrefixError {
    fn from(error: CompiledResourceError) -> Self { Self::Resources(error) }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]''', 1)
s = s.replace('    pub row_payloads: usize,', '''    pub row_payloads: usize,
    pub new_object_slots: usize,
    /// Unique immutable resource entries delivered, not resource bytes.
    pub resource_payloads: usize,''')
s = s.replace('    next_revision: usize,\n    frontier: f64,', '''    next_revision: usize,
    frontier: f64,
    object_slots: usize,
    resources: CompiledResources,''', 1)
s = s.replace('    revisions: Vec<Revision>,\n    stats: PrefixBatchStats,', '''    revisions: Vec<Revision>,
    stats: PrefixBatchStats,
    from_object_slots: usize,
    new_objects: Vec<CompiledObject>,
    resources: CompiledResources,''', 1)
s = s.replace('impl PrefixBatch {\n', '''impl PrefixBatch {
    /// Reserve explicitly before append when additional object storage is needed.
    pub fn required_object_capacity(&self) -> usize {
        self.from_object_slots + self.new_objects.len()
    }
''', 1)
s = s.replace('    limits: ReplayLimits,\n}', '''    limits: ReplayLimits,
    object_capacity: usize,
    initial_object_slots: usize,
}''', 1)
s = s.replace('        self.runtime.seek(time)?;', '        self.runtime.extend_prefix_frame_storage();\n        self.runtime.seek(time)?;', 1)
s = s.replace('''        let history = self
            .runtime
            .replay_history
            .as_mut()
            .expect("private reader history");''', '''        if batch.from_object_slots != self.runtime.compiled.objects().len() {
            return Err(PrefixError::UnexpectedBatch);
        }
        let required = batch.required_object_capacity();
        if required > self.object_capacity {
            return Err(PrefixError::CapacityRequired { object_slots: required });
        }
        self.runtime.compiled.resources().preflight_merge(&batch.resources)?;
        let history = self
            .runtime
            .replay_history
            .as_mut()
            .expect("private reader history");''', 1)
s = s.replace('        history.revisions.extend(batch.revisions);', '''        self.runtime.compiled.merge_prepared_resources(batch.resources);
        self.runtime.compiled.install_prefix_tombstones(batch.new_objects);
        history.revisions.extend(batch.revisions);''', 1)
s = s.replace('            frontier: self.frame.time,\n        };', '''            frontier: self.frame.time,
            object_slots: self.compiled.objects().len(),
            resources: self.compiled.resources().clone(),
        };''', 1)
s = s.replace('            limits: history.limits,\n        };', '''            limits: history.limits,
            object_capacity: self.compiled.objects().len(),
            initial_object_slots: self.compiled.objects().len(),
        };''', 1)
s = s.replace('''        let revisions = suffix
            .iter()''', '''        let new_objects = self.compiled.objects().get(publisher.object_slots..)
            .ok_or(PrefixError::SourceChanged)?.to_vec();
        let resources = self.compiled.prefix_resource_delta(&forward, &publisher.resources)?;
        stats.new_object_slots = new_objects.len();
        stats.resource_payloads = resources.prefix_payload_count();
        stats.payloads = stats.payloads.checked_add(stats.new_object_slots)
            .and_then(|cost| cost.checked_add(stats.resource_payloads))
            .ok_or(ReplayError::RetentionLimit)?;
        if stats.payloads > history.limits.payloads { return Err(ReplayError::RetentionLimit.into()); }
        let revisions = suffix
            .iter()''', 1)
s = s.replace('''            revisions,
            stats,
        };''', '''            revisions,
            stats,
            from_object_slots: publisher.object_slots,
            new_objects,
            resources,
        };''', 1)
s = s.replace('        publisher.next_revision = history.revisions.len();', '''        publisher.resources.extend_prefix_knowledge(&batch.resources);
        publisher.object_slots = self.compiled.objects().len();
        publisher.next_revision = history.revisions.len();''', 1)
p.write_text(s)

write('crates/noon-runtime/src/replay/prefix/storage.rs', '''//! Explicit capacity maintenance; append itself never relocates scene storage.
use super::{PrefixError, PrefixReader, ReplayError};
use crate::{FrameObjectState, SceneInstance};

impl PrefixReader {
    /// Named O(existing storage) maintenance barrier, separate from ordinary
    /// prefix delivery. Reserve ahead using a bounded workload capacity or the
    /// next batch's required_object_capacity(). Returns requested total slots.
    /// Allocation failure changes capacity only, never frame/history/publication.
    /// Payload and revision limits are entry counts, not a physical-memory cap.
    pub fn reserve_object_capacity(&mut self, total: usize) -> Result<usize, PrefixError> {
        if total <= self.object_capacity { return Ok(self.object_capacity); }
        if total > u32::MAX as usize || total.saturating_sub(self.initial_object_slots) > self.limits.payloads {
            return Err(ReplayError::RetentionLimit.into());
        }
        let runtime = &mut self.runtime;
        runtime.compiled.reserve_prefix_object_capacity(total).map_err(|_| PrefixError::Allocation)?;
        macro_rules! reserve {
            ($values:expr) => {
                $values.try_reserve_exact(total.saturating_sub($values.len()))
                    .map_err(|_| PrefixError::Allocation)?;
            }
        }
        reserve!(runtime.frame.objects);
        reserve!(runtime.frame.presences);
        reserve!(runtime.frame.reveals);
        reserve!(runtime.frame.morphs);
        reserve!(runtime.frame.render_geometries);
        reserve!(runtime.frame.render_transforms);
        reserve!(runtime.frame.family_animations);
        reserve!(runtime.frame.family_animation_plan_indices);
        reserve!(runtime.painter_ranks);
        reserve!(runtime.painter_order);
        self.object_capacity = total;
        Ok(total)
    }
}
impl SceneInstance {
    /// Materialize only previously reserved tombstone rows, upon a requested
    /// seek, never during append. Actual membership/content is selected by the
    /// ordinary historical revision exchange before any renderer publication.
    pub(super) fn extend_prefix_frame_storage(&mut self) {
        while self.frame.objects.len() < self.compiled.objects().len() {
            let object = &self.compiled.objects()[self.frame.objects.len()];
            debug_assert!(!object.live);
            self.frame.objects.push(FrameObjectState {
                glow: None, id: object.id, content: object.content.clone(),
                text_bounds: object.text_bounds, transform: object.base_transform,
                spatial: None, style: object.base_style, z_index: object.base_z_index,
                appearance: 1.0,
            });
            self.frame.presences.push(false);
            self.frame.reveals.push(1.0);
            self.frame.morphs.push(0.0);
            self.frame.render_geometries.push(None);
            self.frame.render_transforms.push(None);
            self.frame.family_animations.push(None);
            self.frame.family_animation_plan_indices.push(None);
            self.painter_ranks.push(None);
        }
    }
}
''')

p = Path('crates/noon-runtime/tests/prefix_transfer.rs')
s = p.read_text()
a = s.index('#[test]\nfn unsupported_structural_suffix')
b = s.index('\n#[test]', a + 10)
s = s[:a] + '''#[test]
fn structural_removal_preserves_delivered_history_and_later_membership() {
    let mut source = source(2, limits());
    let (mut publisher, mut reader) = source.create_prefix_reader().unwrap();
    source.apply_execution_patch(&position(3.0)).unwrap();
    source.apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(1))).unwrap();
    source.advance_to(2.0).unwrap();
    reader.append(source.prepare_prefix_update(&mut publisher).unwrap()).unwrap();
    drop(source);
    assert_eq!(reader.seek(1.5).unwrap().objects[0].transform.translation.x, 3.0);
    assert_eq!(reader.take_renderer_publication().painter_order(), &[0]);
    reader.seek(0.5).unwrap();
    assert_eq!(reader.take_renderer_publication().painter_order(), &[0, 1]);
}
''' + s[b:]
p.write_text(s)

replace('docs/architecture.md', '''This initial slice admits fixed membership/content and ordinary property tracks.
An empty reactive runtime installed by semantic lowering is admissible; actual
signals or bindings, including later enrollment, require their own history policy.
Structural/resource extension, family channels, reactive/native input, host
callbacks, terminal endpoint publication and a realtime producer/player shell
remain outside admission.''', '''The experimental 2D slice admits sparse creation, removal/re-entry, order/content
changes and ordinary property tracks. New physical rows are installed as inactive
slots; they become visible only through historical revision selection. Source
resource closures carry only not-yet-delivered immutable handles and dependencies.
A new-slot capacity reservation is an explicit bounded maintenance barrier, never
an implicit whole-scene relocation during append. It may be O(existing storage);
ordinary suffix extraction/installation remains proportional to newly touched
payloads and ordering neighbours. Resource/payload limits count entries, not bytes.
An empty reactive runtime installed by semantic lowering is admissible; actual
signals or bindings, including later enrollment, require their own history policy.
Family channels, spatial/glow plans, reactive/native input, host callbacks,
terminal endpoint publication and a realtime producer/player shell remain outside
admission.''')
print('Lifecycle integration applied; no default feature or viewer changed.')
