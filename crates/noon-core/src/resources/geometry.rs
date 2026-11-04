use std::{
    collections::HashMap,
    mem::size_of_val,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use crate::MeshResource;
use crate::{GeometryId, GeometryRef, Vec2, VectorPath};

static NEXT_GEOMETRY_ARENA: AtomicU64 = AtomicU64::new(1);

fn next_geometry_arena() -> u64 {
    NEXT_GEOMETRY_ARENA
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("geometry arena identity exhausted")
}

const GEOMETRY_RESOURCE_SLOT_BITS: u32 = 32;
const GEOMETRY_RESOURCE_SLOT_MASK: u64 = u32::MAX as u64;

/// Versioned reference to one immutable heavy geometry resource.
///
/// Replacing a resource preserves its stable ID for reconciliation but increments
/// the version so caches and old snapshots cannot silently observe new payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GeometryResourceHandle {
    /// Owning resource namespace; equal slot/version values from another arena are unrelated.
    pub arena: u64,
    pub id: GeometryId,
    pub version: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GeometryResource {
    VectorPath(Arc<VectorPath>),
    Mesh(Arc<MeshResource>),
}

impl GeometryResource {
    pub fn retained_bytes(&self) -> usize {
        match self {
            Self::VectorPath(path) => path.retained_bytes(),
            Self::Mesh(mesh) => mesh.retained_bytes(),
        }
    }
}

/// Geometry reference used by the new semantic/resource boundary.
///
/// Cheap analytic primitives remain inline. Heavy path payloads are always
/// referenced through the resource arena.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StoredGeometry {
    Circle { radius: f32 },
    Rectangle { size: Vec2 },
    Line { start: Vec2, end: Vec2 },
    Resource(GeometryResourceHandle),
}

impl StoredGeometry {
    /// Inline numeric validity; resource liveness is checked against its owning store.
    pub fn is_finite(self) -> bool {
        let point = |p: Vec2| p.x.is_finite() && p.y.is_finite();
        match self {
            Self::Circle { radius } => radius.is_finite(),
            Self::Rectangle { size } => point(size),
            Self::Line { start, end } => point(start) && point(end),
            Self::Resource(_) => true,
        }
    }

    pub const fn resource_handle(self) -> Option<GeometryResourceHandle> {
        match self {
            Self::Resource(handle) => Some(handle),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
struct ResourceEntry {
    generation: u32,
    version: u64,
    value: Option<GeometryResource>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GeometryResourceStats {
    pub live_resources: usize,
    pub retained_bytes: usize,
    pub path_command_bytes: usize,
}

/// Stable-ID arena for immutable geometry payloads.
///
/// `GeometryId` encodes a physical slot plus its generation. This lets the arena
/// reuse storage at the live-working-set high-water mark without allowing a stale
/// bare ID to alias a later occupant of the same slot.
#[derive(Debug)]
pub struct GeometryResourceArena {
    namespace: u64,
    staging_identity: Arc<()>,
    allocation_revision: u64,
    entries: Vec<ResourceEntry>,
    free_slots: Vec<u32>,
    live_resources: usize,
    retained_bytes: usize,
}

impl Clone for GeometryResourceArena {
    fn clone(&self) -> Self {
        Self {
            namespace: self.namespace,
            staging_identity: Arc::new(()),
            allocation_revision: self.allocation_revision,
            entries: self.entries.clone(),
            free_slots: self.free_slots.clone(),
            live_resources: self.live_resources,
            retained_bytes: self.retained_bytes,
        }
    }
}

impl Default for GeometryResourceArena {
    fn default() -> Self {
        Self {
            namespace: next_geometry_arena(),
            staging_identity: Arc::new(()),
            allocation_revision: 0,
            entries: Vec::new(),
            free_slots: Vec::new(),
            live_resources: 0,
            retained_bytes: 0,
        }
    }
}

impl GeometryResourceArena {
    pub(crate) fn fork_namespace(&mut self) -> u64 {
        self.namespace = next_geometry_arena();
        self.namespace
    }

    pub fn new() -> Self {
        Self::default()
    }

    /// Begin a bounded insertion transaction whose handles can be used to
    /// validate a candidate before the resources become visible in this arena.
    pub fn stage_insertions(&self) -> GeometryResourceInsertionStaging {
        GeometryResourceInsertionStaging {
            staging_identity: Arc::clone(&self.staging_identity),
            namespace: self.namespace,
            allocation_revision: self.allocation_revision,
            entry_len: self.entries.len(),
            free_slot_len: self.free_slots.len(),
            reused_slots: 0,
            retained_bytes: 0,
            entries: Vec::new(),
            handle_indices: HashMap::new(),
        }
    }

    pub fn insert_path(&mut self, path: impl Into<Arc<VectorPath>>) -> GeometryResourceHandle {
        self.insert(GeometryResource::VectorPath(path.into()))
    }

    pub fn insert_mesh(&mut self, mesh: MeshResource) -> GeometryResourceHandle {
        self.insert(GeometryResource::Mesh(Arc::new(mesh)))
    }

    pub fn insert(&mut self, resource: GeometryResource) -> GeometryResourceHandle {
        self.advance_allocation_revision();
        let resource_bytes = resource.retained_bytes();
        let handle = if let Some(slot) = self.free_slots.pop() {
            let index = slot as usize;
            let entry = &mut self.entries[index];
            debug_assert!(entry.value.is_none());
            entry.version = 0;
            entry.value = Some(resource);
            GeometryResourceHandle {
                arena: self.namespace,
                id: geometry_resource_id(index, entry.generation),
                version: 0,
            }
        } else {
            let index = self.entries.len();
            let id = geometry_resource_id(index, 0);
            self.entries.push(ResourceEntry {
                generation: 0,
                version: 0,
                value: Some(resource),
            });
            GeometryResourceHandle {
                arena: self.namespace,
                id,
                version: 0,
            }
        };

        self.retained_bytes = self.retained_bytes.saturating_add(resource_bytes);
        self.live_resources += 1;
        handle
    }

    /// Promote a legacy geometry value to the new storage model.
    ///
    /// Paths are cloned exactly once into arena ownership. Every later semantic,
    /// track, compiled, or frame snapshot can copy the returned small handle.
    pub fn promote(
        &mut self,
        geometry: &GeometryRef,
    ) -> Result<StoredGeometry, GeometryResourceError> {
        Ok(match geometry {
            GeometryRef::Circle { radius } => StoredGeometry::Circle { radius: *radius },
            GeometryRef::Rectangle { size } => StoredGeometry::Rectangle { size: *size },
            GeometryRef::Line { start, end } => StoredGeometry::Line {
                start: *start,
                end: *end,
            },
            GeometryRef::VectorPath(path) => {
                StoredGeometry::Resource(self.insert(GeometryResource::VectorPath(path.clone())))
            }
            GeometryRef::External(id) => {
                let handle = self
                    .current_handle(*id)
                    .ok_or(GeometryResourceError::UnknownResource(*id))?;
                StoredGeometry::Resource(handle)
            }
        })
    }

    pub fn get(&self, handle: GeometryResourceHandle) -> Option<&GeometryResource> {
        if handle.arena != self.namespace {
            return None;
        }
        let index = geometry_resource_slot(handle.id);
        let entry = self.entries.get(index)?;
        if geometry_resource_id(index, entry.generation) != handle.id
            || entry.version != handle.version
        {
            return None;
        }
        entry.value.as_ref()
    }

    pub fn current_handle(&self, id: GeometryId) -> Option<GeometryResourceHandle> {
        let index = geometry_resource_slot(id);
        let entry = self.entries.get(index)?;
        if geometry_resource_id(index, entry.generation) != id {
            return None;
        }
        entry.value.as_ref()?;
        Some(GeometryResourceHandle {
            arena: self.namespace,
            id,
            version: entry.version,
        })
    }

    /// Replace one immutable payload and invalidate every old versioned handle.
    pub fn replace(
        &mut self,
        id: GeometryId,
        resource: GeometryResource,
    ) -> Result<GeometryResourceHandle, GeometryResourceError> {
        let index = geometry_resource_slot(id);
        let entry = self
            .entries
            .get_mut(index)
            .filter(|entry| geometry_resource_id(index, entry.generation) == id)
            .ok_or(GeometryResourceError::UnknownResource(id))?;
        let previous = entry
            .value
            .as_ref()
            .ok_or(GeometryResourceError::UnknownResource(id))?;
        let next_version = entry
            .version
            .checked_add(1)
            .ok_or(GeometryResourceError::VersionExhausted(id))?;
        self.retained_bytes = self
            .retained_bytes
            .saturating_sub(previous.retained_bytes());
        self.retained_bytes = self
            .retained_bytes
            .saturating_add(resource.retained_bytes());
        entry.version = next_version;
        entry.value = Some(resource);
        Ok(GeometryResourceHandle {
            arena: self.namespace,
            id,
            version: entry.version,
        })
    }

    pub fn remove(&mut self, id: GeometryId) -> Result<GeometryResource, GeometryResourceError> {
        let index = geometry_resource_slot(id);
        let (next_version, previous_retained_bytes) = {
            let entry = self
                .entries
                .get(index)
                .filter(|entry| geometry_resource_id(index, entry.generation) == id)
                .ok_or(GeometryResourceError::UnknownResource(id))?;
            let previous = entry
                .value
                .as_ref()
                .ok_or(GeometryResourceError::UnknownResource(id))?;
            let next_version = entry
                .version
                .checked_add(1)
                .ok_or(GeometryResourceError::VersionExhausted(id))?;
            (next_version, previous.retained_bytes())
        };
        let next_revision = self
            .allocation_revision
            .checked_add(1)
            .ok_or(GeometryResourceError::RevisionExhausted)?;
        self.allocation_revision = next_revision;
        let entry = &mut self.entries[index];
        let resource = entry
            .value
            .take()
            .expect("resource presence was validated before mutation");
        self.retained_bytes = self.retained_bytes.saturating_sub(previous_retained_bytes);
        self.live_resources -= 1;
        entry.version = next_version;

        // Generation wrap must never make an old bare GeometryId valid again. A
        // fully exhausted physical slot is therefore retired instead of recycled.
        if let Some(next_generation) = entry.generation.checked_add(1) {
            entry.generation = next_generation;
            self.free_slots
                .push(u32::try_from(index).expect("Noon geometry resource slot space exhausted"));
        }

        Ok(resource)
    }

    pub fn stats(&self) -> GeometryResourceStats {
        GeometryResourceStats {
            live_resources: self.live_resources,
            retained_bytes: self.retained_bytes,
            path_command_bytes: self
                .entries
                .iter()
                .filter_map(|entry| entry.value.as_ref())
                .map(|resource| match resource {
                    GeometryResource::VectorPath(path) => size_of_val(path.commands()),
                    GeometryResource::Mesh(_) => 0,
                })
                .sum(),
        }
    }

    /// Number of physical arena slots retained at the current high-water mark.
    pub fn slot_capacity(&self) -> usize {
        self.entries.len()
    }

    pub fn len(&self) -> usize {
        self.live_resources
    }

    pub fn is_empty(&self) -> bool {
        self.live_resources == 0
    }

    /// Reserve for staged entries before a larger candidate is published.
    /// This may change vector capacity, but never changes handles or live data.
    pub fn reserve_insertions(
        &mut self,
        staged: &GeometryResourceInsertionStaging,
    ) -> Result<(), GeometryResourceError> {
        let (_, _, appended, _) = self.staged_insertion_totals(staged)?;
        self.entries
            .try_reserve(appended)
            .map_err(|_| GeometryResourceError::AllocationFailed)
    }

    /// Publish all staged insertions after checking that their planned slots
    /// still belong to this unchanged arena. Validation and reservation finish
    /// before any entry, free-list, or accounting mutation.
    pub fn commit_insertions(
        &mut self,
        staged: GeometryResourceInsertionStaging,
    ) -> Result<(), GeometryResourceError> {
        let (live_resources, retained_bytes, appended, next_revision) =
            self.staged_insertion_totals(&staged)?;
        if staged.entries.is_empty() {
            return Ok(());
        }
        self.entries
            .try_reserve(appended)
            .map_err(|_| GeometryResourceError::AllocationFailed)?;
        for (handle, resource) in staged.entries {
            let slot = geometry_resource_slot(handle.id);
            if slot < staged.entry_len {
                let free_slot = self.free_slots.pop();
                debug_assert_eq!(free_slot, Some(slot as u32));
                let entry = &mut self.entries[slot];
                debug_assert!(entry.value.is_none());
                debug_assert_eq!(entry.generation, (handle.id.get() >> 32) as u32);
                entry.version = 0;
                entry.value = Some(resource);
            } else {
                debug_assert_eq!(slot, self.entries.len());
                self.entries.push(ResourceEntry {
                    generation: 0,
                    version: 0,
                    value: Some(resource),
                });
            }
        }
        self.live_resources = live_resources;
        self.retained_bytes = retained_bytes;
        self.allocation_revision = next_revision;
        Ok(())
    }

    fn staged_insertion_totals(
        &self,
        staged: &GeometryResourceInsertionStaging,
    ) -> Result<(usize, usize, usize, u64), GeometryResourceError> {
        staged.validate_base(self)?;
        let live_resources = self
            .live_resources
            .checked_add(staged.entries.len())
            .ok_or(GeometryResourceError::ResourceCountOverflow)?;
        let retained_bytes = self
            .retained_bytes
            .checked_add(staged.retained_bytes)
            .ok_or(GeometryResourceError::RetainedBytesOverflow)?;
        let appended = staged.entries.len() - staged.reused_slots;
        let revision = if staged.entries.is_empty() {
            self.allocation_revision
        } else {
            self.allocation_revision
                .checked_add(1)
                .ok_or(GeometryResourceError::RevisionExhausted)?
        };
        Ok((live_resources, retained_bytes, appended, revision))
    }

    fn advance_allocation_revision(&mut self) {
        self.allocation_revision = self
            .allocation_revision
            .checked_add(1)
            .expect("geometry resource arena revision exhausted");
    }
}

/// Unpublished geometry insertions with handles derived from one arena's next
/// allocation state. The staging value owns only its added resources.
#[derive(Debug)]
pub struct GeometryResourceInsertionStaging {
    staging_identity: Arc<()>,
    namespace: u64,
    allocation_revision: u64,
    entry_len: usize,
    free_slot_len: usize,
    reused_slots: usize,
    retained_bytes: usize,
    entries: Vec<(GeometryResourceHandle, GeometryResource)>,
    handle_indices: HashMap<GeometryResourceHandle, usize>,
}

impl GeometryResourceInsertionStaging {
    /// Stage one resource and return the exact handle it will receive on commit.
    pub fn insert(
        &mut self,
        arena: &GeometryResourceArena,
        resource: GeometryResource,
    ) -> Result<GeometryResourceHandle, GeometryResourceError> {
        self.validate_base(arena)?;
        let total_bytes = self
            .retained_bytes
            .checked_add(resource.retained_bytes())
            .ok_or(GeometryResourceError::RetainedBytesOverflow)?;
        let reuses_slot = self.reused_slots < self.free_slot_len;
        let (slot, generation) = if reuses_slot {
            let free_index = self.free_slot_len - self.reused_slots - 1;
            let slot = *arena
                .free_slots
                .get(free_index)
                .ok_or(GeometryResourceError::StaleStaging)?;
            let generation = arena
                .entries
                .get(slot as usize)
                .filter(|entry| entry.value.is_none())
                .map(|entry| entry.generation)
                .ok_or(GeometryResourceError::StaleStaging)?;
            (slot as usize, generation)
        } else {
            let slot = self
                .entry_len
                .checked_add(self.entries.len() - self.reused_slots)
                .ok_or(GeometryResourceError::ResourceSlotExhausted)?;
            (slot, 0)
        };
        u32::try_from(slot).map_err(|_| GeometryResourceError::ResourceSlotExhausted)?;
        let id = geometry_resource_id(slot, generation);
        let handle = GeometryResourceHandle {
            arena: self.namespace,
            id,
            version: 0,
        };
        let index = self.entries.len();
        self.entries
            .try_reserve(1)
            .map_err(|_| GeometryResourceError::AllocationFailed)?;
        self.handle_indices
            .try_reserve(1)
            .map_err(|_| GeometryResourceError::AllocationFailed)?;
        self.entries.push((handle, resource));
        self.handle_indices.insert(handle, index);
        self.reused_slots += usize::from(reuses_slot);
        self.retained_bytes = total_bytes;
        Ok(handle)
    }

    /// Read a resource staged by this transaction, without publishing it.
    pub fn get(&self, handle: GeometryResourceHandle) -> Option<&GeometryResource> {
        self.handle_indices
            .get(&handle)
            .and_then(|index| self.entries.get(*index))
            .map(|(_, resource)| resource)
    }

    fn validate_base(&self, arena: &GeometryResourceArena) -> Result<(), GeometryResourceError> {
        if !Arc::ptr_eq(&self.staging_identity, &arena.staging_identity)
            || self.namespace != arena.namespace
        {
            return Err(GeometryResourceError::ForeignStaging);
        }
        if self.allocation_revision != arena.allocation_revision
            || self.entry_len != arena.entries.len()
            || self.free_slot_len != arena.free_slots.len()
        {
            return Err(GeometryResourceError::StaleStaging);
        }
        Ok(())
    }
}

fn geometry_resource_id(slot: usize, generation: u32) -> GeometryId {
    let slot = u32::try_from(slot).expect("Noon geometry resource slot space exhausted");
    GeometryId::new((u64::from(generation) << GEOMETRY_RESOURCE_SLOT_BITS) | u64::from(slot))
}

fn geometry_resource_slot(id: GeometryId) -> usize {
    (id.get() & GEOMETRY_RESOURCE_SLOT_MASK) as usize
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryResourceError {
    /// A path rejected before resource allocation contains non-finite points.
    NonFinitePath,
    UnknownResource(GeometryId),
    VersionExhausted(GeometryId),
    RevisionExhausted,
    ForeignStaging,
    StaleStaging,
    ResourceSlotExhausted,
    ResourceCountOverflow,
    RetainedBytesOverflow,
    AllocationFailed,
}

impl std::fmt::Display for GeometryResourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFinitePath => formatter.write_str("geometry path contains non-finite points"),
            Self::UnknownResource(id) => {
                write!(formatter, "unknown geometry resource {}", id.get())
            }
            Self::VersionExhausted(id) => {
                write!(
                    formatter,
                    "geometry resource {} version space exhausted",
                    id.get()
                )
            }
            Self::RevisionExhausted => {
                formatter.write_str("geometry resource arena revision exhausted")
            }
            Self::ForeignStaging => formatter.write_str("geometry insertion staging is foreign"),
            Self::StaleStaging => formatter.write_str("geometry insertion staging is stale"),
            Self::ResourceSlotExhausted => {
                formatter.write_str("geometry resource slot space exhausted")
            }
            Self::ResourceCountOverflow => formatter.write_str("geometry resource count overflow"),
            Self::RetainedBytesOverflow => {
                formatter.write_str("geometry resource retained byte count overflow")
            }
            Self::AllocationFailed => formatter.write_str("geometry resource allocation failed"),
        }
    }
}

impl std::error::Error for GeometryResourceError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    fn large_path(count: usize) -> VectorPath {
        let mut path = VectorPath::new().move_to(Vec2::ZERO);
        for index in 0..count {
            path = path.line_to(Vec2::new(index as f32 * 0.01, (index % 7) as f32));
        }
        path
    }

    fn mesh_resource(offset: f32) -> GeometryResource {
        GeometryResource::Mesh(Arc::new(
            MeshResource::new(
                vec![
                    crate::SemanticVec3::new(f64::from(offset), 0.0, 0.0),
                    crate::SemanticVec3::new(f64::from(offset + 1.0), 0.0, 0.0),
                    crate::SemanticVec3::new(f64::from(offset), 1.0, 0.0),
                ],
                None,
                vec![0, 1, 2],
            )
            .unwrap(),
        ))
    }

    #[test]
    fn staged_mesh_insertions_resolve_before_commit_and_reuse_planned_free_slots() {
        let mut arena = GeometryResourceArena::new();
        let first = arena.insert(mesh_resource(0.0));
        let second = arena.insert(mesh_resource(2.0));
        arena.remove(first.id).unwrap();
        arena.remove(second.id).unwrap();
        let capacity = arena.slot_capacity();

        let mut staged = arena.stage_insertions();
        let next_resource = mesh_resource(4.0);
        let GeometryResource::Mesh(shared) = &next_resource else {
            unreachable!();
        };
        let shared = Arc::clone(shared);
        let next = staged.insert(&arena, next_resource).unwrap();
        let last = staged.insert(&arena, mesh_resource(6.0)).unwrap();
        assert_ne!(next.id, last.id);
        let before_reservation = arena.stats();
        arena.reserve_insertions(&staged).unwrap();
        assert_eq!(arena.stats(), before_reservation);
        assert!(arena.get(next).is_none());
        assert!(arena.get(last).is_none());
        assert!(matches!(staged.get(next), Some(GeometryResource::Mesh(_))));
        assert!(matches!(staged.get(last), Some(GeometryResource::Mesh(_))));
        let Some(GeometryResource::Mesh(staged_mesh)) = staged.get(next) else {
            unreachable!();
        };
        assert!(Arc::ptr_eq(&shared, staged_mesh));

        arena.commit_insertions(staged).unwrap();
        assert_eq!(arena.slot_capacity(), capacity);
        assert!(arena.get(next).is_some());
        assert!(arena.get(last).is_some());
        let Some(GeometryResource::Mesh(committed_mesh)) = arena.get(next) else {
            unreachable!();
        };
        assert!(Arc::ptr_eq(&shared, committed_mesh));
        assert_eq!(arena.current_handle(first.id), None);
        assert_eq!(arena.current_handle(second.id), None);
    }

    #[test]
    fn staged_mesh_insertions_append_after_any_available_free_slots() {
        for free_slot in [false, true] {
            let mut arena = GeometryResourceArena::new();
            if free_slot {
                let removed = arena.insert(mesh_resource(0.0));
                arena.remove(removed.id).unwrap();
            }
            let mut staged = arena.stage_insertions();
            let first = staged.insert(&arena, mesh_resource(2.0)).unwrap();
            let second = staged.insert(&arena, mesh_resource(4.0)).unwrap();
            assert_ne!(first.id, second.id);
            arena.reserve_insertions(&staged).unwrap();
            arena.commit_insertions(staged).unwrap();
            assert!(arena.get(first).is_some());
            assert!(arena.get(second).is_some());
            assert_eq!(arena.len(), 2);
            assert_eq!(arena.slot_capacity(), 2);
        }
    }

    #[test]
    fn foreign_and_stale_staged_insertions_reject_without_commit_mutation() {
        let arena = GeometryResourceArena::new();
        let mut foreign = arena.stage_insertions();
        let foreign_handle = foreign.insert(&arena, mesh_resource(0.0)).unwrap();
        let mut clone = arena.clone();
        let clone_before = clone.stats();
        assert_eq!(
            clone.commit_insertions(foreign),
            Err(GeometryResourceError::ForeignStaging)
        );
        assert_eq!(clone.stats(), clone_before);
        assert!(clone.get(foreign_handle).is_none());

        let mut stale_arena = GeometryResourceArena::new();
        let mut staged = stale_arena.stage_insertions();
        let staged_handle = staged.insert(&stale_arena, mesh_resource(4.0)).unwrap();
        let intervening = stale_arena.insert(mesh_resource(2.0));
        stale_arena.remove(intervening.id).unwrap();
        let before_commit = stale_arena.stats();

        assert_eq!(
            stale_arena.commit_insertions(staged),
            Err(GeometryResourceError::StaleStaging)
        );
        assert_eq!(stale_arena.stats(), before_commit);
        assert_eq!(stale_arena.current_handle(staged_handle.id), None);
    }

    #[test]
    fn raw_coordinate_retention_counts_singleton_incomplete_and_complete_arrays() {
        for count in [0, 1, 2, 3, 4, 5, 7, 8, 9] {
            let path = VectorPath::from_manim_points(vec![Vec2::new(1.0, 2.0); count]);
            let expected = size_of::<VectorPath>()
                + size_of_val(path.commands())
                + path.manim_points().map_or(0, size_of_val);
            let mut arena = GeometryResourceArena::new();
            arena.insert_path(path);
            assert_eq!(arena.stats().retained_bytes, expected, "{count} raw points");
        }
    }

    #[test]
    fn raw_coordinate_retention_includes_each_morph_payload_once() {
        let target = VectorPath::from_manim_points(vec![Vec2::ZERO; 5]);
        let source = VectorPath::from_manim_points(vec![Vec2::ZERO; 3]);
        let expected = 2 * size_of::<VectorPath>()
            + size_of_val(source.commands())
            + size_of_val(source.manim_points().unwrap())
            + size_of_val(target.commands())
            + size_of_val(target.manim_points().unwrap());
        let mut arena = GeometryResourceArena::new();
        arena.insert_path(source.with_morph_target(target));
        assert_eq!(arena.stats().retained_bytes, expected);
    }

    #[test]
    fn raw_coordinate_retention_replacement_and_removal_preserve_unrelated_resources() {
        let short = VectorPath::from_manim_points(vec![Vec2::ZERO]);
        let long = VectorPath::from_manim_points(vec![Vec2::ZERO; 3]);
        assert_eq!(short.commands(), long.commands());
        let extra_raw_bytes = size_of_val(&long.manim_points().unwrap()[1..]);
        assert!(extra_raw_bytes > 0);
        let mut arena = GeometryResourceArena::new();
        let unrelated = arena.insert_path(large_path(2));
        let unrelated_stats = arena.stats();
        let old = arena.insert_path(short);
        let old_stats = arena.stats();
        let new = arena
            .replace(old.id, GeometryResource::VectorPath(Arc::new(long)))
            .unwrap();
        assert_eq!(
            arena.stats().retained_bytes,
            old_stats.retained_bytes + extra_raw_bytes
        );
        assert_eq!(
            arena.stats().path_command_bytes,
            old_stats.path_command_bytes
        );
        assert!(arena.get(old).is_none());
        assert!(arena.get(new).is_some());
        assert!(arena.get(unrelated).is_some());
        arena.remove(new.id).unwrap();
        assert_eq!(arena.stats(), unrelated_stats);
        assert!(arena.get(unrelated).is_some());
    }

    #[test]
    fn heavy_path_is_owned_once_and_shared_by_small_handles() {
        let mut arena = GeometryResourceArena::new();
        let handle = arena.insert_path(large_path(10_000));
        let references = vec![StoredGeometry::Resource(handle); 100_000];
        assert_eq!(references.len(), 100_000);
        assert_eq!(arena.len(), 1);
        let GeometryResource::VectorPath(path) = arena.get(handle).unwrap() else {
            panic!("expected path")
        };
        assert_eq!(path.commands().len(), 10_001);
        assert!(arena.stats().retained_bytes > 0);
    }

    #[test]
    fn analytic_primitives_remain_inline() {
        let mut arena = GeometryResourceArena::new();
        assert_eq!(
            arena.promote(&GeometryRef::circle(2.0)).unwrap(),
            StoredGeometry::Circle { radius: 2.0 }
        );
        assert!(arena.is_empty());
    }

    #[test]
    fn mesh_resources_share_arc_payload_and_follow_arena_replacement_and_reuse() {
        let mesh = MeshResource::new(
            vec![
                crate::SemanticVec3::ZERO,
                crate::SemanticVec3::new(1.0, 0.0, 0.0),
                crate::SemanticVec3::new(0.0, 1.0, 2.0),
            ],
            None,
            vec![0, 1, 2],
        )
        .unwrap();
        let retained = mesh.retained_bytes();
        let mut arena = GeometryResourceArena::new();
        let first = arena.insert_mesh(mesh.clone());
        let GeometryResource::Mesh(first_arc) = arena.get(first).unwrap() else {
            panic!("expected mesh")
        };
        let shared = first_arc.clone();
        assert!(Arc::ptr_eq(first_arc, &shared));
        assert_eq!(arena.stats().retained_bytes, retained);
        assert_eq!(arena.stats().path_command_bytes, 0);

        let second = arena
            .replace(first.id, GeometryResource::Mesh(Arc::new(mesh)))
            .unwrap();
        assert!(arena.get(first).is_none());
        assert!(matches!(arena.get(second), Some(GeometryResource::Mesh(_))));
        arena.remove(second.id).unwrap();
        let recycled = arena.insert_mesh(
            MeshResource::new(
                vec![
                    crate::SemanticVec3::ZERO,
                    crate::SemanticVec3::new(1.0, 0.0, 0.0),
                    crate::SemanticVec3::new(0.0, 1.0, 0.0),
                ],
                None,
                vec![0, 1, 2],
            )
            .unwrap(),
        );
        assert_eq!(arena.slot_capacity(), 1);
        assert_ne!(second.id, recycled.id);
        assert!(arena.get(second).is_none());
    }

    #[test]
    fn replace_preserves_id_but_invalidates_old_version() {
        let mut arena = GeometryResourceArena::new();
        let first = arena.insert_path(large_path(10));
        let second = arena
            .replace(
                first.id,
                GeometryResource::VectorPath(Arc::new(large_path(20))),
            )
            .unwrap();
        assert_eq!(first.id, second.id);
        assert_ne!(first.version, second.version);
        assert!(arena.get(first).is_none());
        assert!(arena.get(second).is_some());
    }

    #[test]
    fn removal_invalidates_resource_without_renumbering_other_ids() {
        let mut arena = GeometryResourceArena::new();
        let first = arena.insert_path(large_path(4));
        let second = arena.insert_path(large_path(8));
        arena.remove(first.id).unwrap();
        assert!(arena.get(first).is_none());
        assert_eq!(arena.current_handle(second.id), Some(second));
    }

    #[test]
    fn recycled_slot_gets_new_id_and_rejects_stale_bare_id() {
        let mut arena = GeometryResourceArena::new();
        let first = arena.insert_path(large_path(4));
        let original_capacity = arena.slot_capacity();
        arena.remove(first.id).unwrap();
        let second = arena.insert_path(large_path(8));

        assert_eq!(original_capacity, 1);
        assert_eq!(arena.slot_capacity(), original_capacity);
        assert_ne!(first.id, second.id);
        assert_eq!(
            geometry_resource_slot(first.id),
            geometry_resource_slot(second.id)
        );
        assert!(arena.get(first).is_none());
        assert_eq!(arena.current_handle(first.id), None);
        assert_eq!(
            arena.replace(
                first.id,
                GeometryResource::VectorPath(Arc::new(large_path(16))),
            ),
            Err(GeometryResourceError::UnknownResource(first.id)),
        );
        assert_eq!(
            arena.remove(first.id),
            Err(GeometryResourceError::UnknownResource(first.id)),
        );
        assert!(arena.get(second).is_some());
    }

    #[test]
    fn repeated_remove_insert_churn_reuses_one_physical_slot() {
        let mut arena = GeometryResourceArena::new();
        let mut current = arena.insert_path(large_path(2));

        for index in 0..1_000 {
            let stale = current;
            arena.remove(stale.id).unwrap();
            current = arena.insert_path(large_path(2 + (index % 3)));

            assert_eq!(arena.len(), 1);
            assert_eq!(arena.slot_capacity(), 1);
            assert_ne!(current.id, stale.id);
            assert!(arena.get(stale).is_none());
            assert_eq!(arena.current_handle(stale.id), None);
            assert!(arena.get(current).is_some());
        }
    }

    #[test]
    fn generation_exhaustion_retires_slot_instead_of_aliasing() {
        let mut arena = GeometryResourceArena::new();
        let first = arena.insert_path(large_path(4));
        let slot = geometry_resource_slot(first.id);
        arena.entries[slot].generation = u32::MAX;
        let exhausted_id = geometry_resource_id(slot, u32::MAX);
        let exhausted = GeometryResourceHandle {
            arena: first.arena,
            id: exhausted_id,
            version: first.version,
        };

        arena.remove(exhausted_id).unwrap();
        let replacement = arena.insert_path(large_path(8));

        assert_eq!(arena.slot_capacity(), 2);
        assert_ne!(replacement.id, exhausted.id);
        assert!(arena.get(exhausted).is_none());
        assert_eq!(arena.current_handle(exhausted.id), None);
    }

    #[test]
    fn replace_version_exhaustion_leaves_resource_and_accounting_unchanged() {
        let mut arena = GeometryResourceArena::new();
        let first = arena.insert_path(large_path(4));
        let entry = &mut arena.entries[geometry_resource_slot(first.id)];
        entry.version = u64::MAX;
        let exhausted = GeometryResourceHandle {
            arena: first.arena,
            id: first.id,
            version: u64::MAX,
        };
        let before = arena.stats();
        let GeometryResource::VectorPath(before_path) = arena.get(exhausted).unwrap() else {
            panic!("expected path")
        };
        let before_commands = before_path.commands().len();

        assert_eq!(
            arena.replace(
                first.id,
                GeometryResource::VectorPath(Arc::new(large_path(40))),
            ),
            Err(GeometryResourceError::VersionExhausted(first.id)),
        );

        assert_eq!(arena.stats(), before);
        assert_eq!(arena.current_handle(first.id), Some(exhausted));
        let GeometryResource::VectorPath(after_path) = arena.get(exhausted).unwrap() else {
            panic!("expected path")
        };
        assert_eq!(after_path.commands().len(), before_commands);
    }

    #[test]
    fn remove_version_exhaustion_leaves_resource_and_accounting_unchanged() {
        let mut arena = GeometryResourceArena::new();
        let first = arena.insert_path(large_path(12));
        let entry = &mut arena.entries[geometry_resource_slot(first.id)];
        entry.version = u64::MAX;
        let exhausted = GeometryResourceHandle {
            arena: first.arena,
            id: first.id,
            version: u64::MAX,
        };
        let before = arena.stats();

        assert_eq!(
            arena.remove(first.id),
            Err(GeometryResourceError::VersionExhausted(first.id)),
        );

        assert_eq!(arena.stats(), before);
        assert_eq!(arena.len(), 1);
        assert_eq!(arena.current_handle(first.id), Some(exhausted));
        assert!(arena.get(exhausted).is_some());
    }

    #[test]
    fn promoting_vector_path_moves_future_snapshots_to_handle_semantics() {
        let path = large_path(1_000);
        let legacy = GeometryRef::path(path);
        let mut arena = GeometryResourceArena::new();
        let stored = arena.promote(&legacy).unwrap();
        let handle = stored.resource_handle().unwrap();
        let snapshots = vec![stored; 50_000];
        assert!(snapshots
            .iter()
            .all(|snapshot| snapshot.resource_handle() == Some(handle)));
        assert_eq!(arena.len(), 1);
    }
}
