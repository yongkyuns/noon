use std::{
    mem::{size_of, size_of_val},
    sync::Arc,
};

use crate::{GeometryId, GeometryRef, Vec2, VectorPath};

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
#[derive(Clone, Debug, Default)]
pub struct GeometryResourceArena {
    entries: Vec<ResourceEntry>,
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
        let id = GeometryId::new(
            u64::try_from(self.entries.len()).expect("Noon geometry resource ID space exhausted"),
        );
        self.retained_bytes = self
            .retained_bytes
            .saturating_add(resource.retained_bytes());
        self.entries.push(ResourceEntry {
            version: 0,
            value: Some(resource),
        });
        self.live_resources += 1;
        GeometryResourceHandle { id, version: 0 }
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
        let entry = self.entries.get(handle.id.get() as usize)?;
        if entry.version != handle.version {
            return None;
        }
        entry.value.as_ref()
    }

    pub fn current_handle(&self, id: GeometryId) -> Option<GeometryResourceHandle> {
        let entry = self.entries.get(id.get() as usize)?;
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
        let entry = self
            .entries
            .get_mut(id.get() as usize)
            .ok_or(GeometryResourceError::UnknownResource(id))?;
        let previous = entry
            .value
            .as_ref()
            .ok_or(GeometryResourceError::UnknownResource(id))?;
        self.retained_bytes = self
            .retained_bytes
            .saturating_sub(previous.retained_bytes());
        self.retained_bytes = self
            .retained_bytes
            .saturating_add(resource.retained_bytes());
        entry.version = entry
            .version
            .checked_add(1)
            .ok_or(GeometryResourceError::VersionExhausted(id))?;
        entry.value = Some(resource);
        Ok(GeometryResourceHandle {
            id,
            version: entry.version,
        })
    }

    pub fn remove(&mut self, id: GeometryId) -> Result<GeometryResource, GeometryResourceError> {
        let entry = self
            .entries
            .get_mut(id.get() as usize)
            .ok_or(GeometryResourceError::UnknownResource(id))?;
        let resource = entry
            .value
            .take()
            .ok_or(GeometryResourceError::UnknownResource(id))?;
        self.retained_bytes = self
            .retained_bytes
            .saturating_sub(resource.retained_bytes());
        self.live_resources -= 1;
        entry.version = entry
            .version
            .checked_add(1)
            .ok_or(GeometryResourceError::VersionExhausted(id))?;
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

    pub fn len(&self) -> usize {
        self.live_resources
    }

    pub fn is_empty(&self) -> bool {
        self.live_resources == 0
    }
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
