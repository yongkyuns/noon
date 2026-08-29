use std::{
    mem::{size_of, size_of_val},
    sync::Arc,
};

use crate::{GeometryId, GeometryRef, Vec2, VectorPath};

const GEOMETRY_RESOURCE_SLOT_BITS: u32 = 32;
const GEOMETRY_RESOURCE_SLOT_MASK: u64 = u32::MAX as u64;

/// Versioned reference to one immutable heavy geometry resource.
///
/// Replacing a resource preserves its stable ID for reconciliation but increments
/// the version so caches and old snapshots cannot silently observe new payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GeometryResourceHandle {
    pub id: GeometryId,
    pub version: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GeometryResource {
    VectorPath(Arc<VectorPath>),
}

impl GeometryResource {
    pub fn retained_bytes(&self) -> usize {
        match self {
            Self::VectorPath(path) => vector_path_retained_bytes(path),
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
#[derive(Clone, Debug, Default)]
pub struct GeometryResourceArena {
    entries: Vec<ResourceEntry>,
    free_slots: Vec<u32>,
    live_resources: usize,
    retained_bytes: usize,
}

impl GeometryResourceArena {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_path(&mut self, path: VectorPath) -> GeometryResourceHandle {
        self.insert(GeometryResource::VectorPath(Arc::new(path)))
    }

    pub fn insert(&mut self, resource: GeometryResource) -> GeometryResourceHandle {
        let resource_bytes = resource.retained_bytes();
        let handle = if let Some(slot) = self.free_slots.pop() {
            let index = slot as usize;
            let entry = &mut self.entries[index];
            debug_assert!(entry.value.is_none());
            entry.version = 0;
            entry.value = Some(resource);
            GeometryResourceHandle {
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
            GeometryResourceHandle { id, version: 0 }
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
                StoredGeometry::Resource(self.insert_path(path.clone()))
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
            id,
            version: entry.version,
        })
    }

    pub fn remove(&mut self, id: GeometryId) -> Result<GeometryResource, GeometryResourceError> {
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
        let previous_retained_bytes = previous.retained_bytes();
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
}

fn geometry_resource_id(slot: usize, generation: u32) -> GeometryId {
    let slot = u32::try_from(slot).expect("Noon geometry resource slot space exhausted");
    GeometryId::new((u64::from(generation) << GEOMETRY_RESOURCE_SLOT_BITS) | u64::from(slot))
}

fn geometry_resource_slot(id: GeometryId) -> usize {
    (id.get() & GEOMETRY_RESOURCE_SLOT_MASK) as usize
}

fn vector_path_retained_bytes(path: &VectorPath) -> usize {
    // `VectorPath` owns a command Vec and may recursively own a morph target.
    // Count command payload capacity approximately by logical length; allocator
    // bookkeeping is deliberately excluded from deterministic instrumentation.
    let own = size_of::<VectorPath>() + size_of_val(path.commands());
    own + path
        .morph_target()
        .map(vector_path_retained_bytes)
        .unwrap_or(0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryResourceError {
    UnknownResource(GeometryId),
    VersionExhausted(GeometryId),
}

impl std::fmt::Display for GeometryResourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
        }
    }
}

impl std::error::Error for GeometryResourceError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn large_path(count: usize) -> VectorPath {
        let mut path = VectorPath::new().move_to(Vec2::ZERO);
        for index in 0..count {
            path = path.line_to(Vec2::new(index as f32 * 0.01, (index % 7) as f32));
        }
        path
    }

    #[test]
    fn heavy_path_is_owned_once_and_shared_by_small_handles() {
        let mut arena = GeometryResourceArena::new();
        let handle = arena.insert_path(large_path(10_000));
        let references = vec![StoredGeometry::Resource(handle); 100_000];
        assert_eq!(references.len(), 100_000);
        assert_eq!(arena.len(), 1);
        let GeometryResource::VectorPath(path) = arena.get(handle).unwrap();
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
            id: first.id,
            version: u64::MAX,
        };
        let before = arena.stats();
        let GeometryResource::VectorPath(before_path) = arena.get(exhausted).unwrap();
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
        let GeometryResource::VectorPath(after_path) = arena.get(exhausted).unwrap();
        assert_eq!(after_path.commands().len(), before_commands);
    }

    #[test]
    fn remove_version_exhaustion_leaves_resource_and_accounting_unchanged() {
        let mut arena = GeometryResourceArena::new();
        let first = arena.insert_path(large_path(12));
        let entry = &mut arena.entries[geometry_resource_slot(first.id)];
        entry.version = u64::MAX;
        let exhausted = GeometryResourceHandle {
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
