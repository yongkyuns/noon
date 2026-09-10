use std::{
    collections::HashMap,
    mem::size_of,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

static NEXT_RASTER_IMAGE_RESOURCE_ARENA: AtomicU64 = AtomicU64::new(1);

fn next_raster_image_resource_arena() -> u64 {
    NEXT_RASTER_IMAGE_RESOURCE_ARENA
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("raster image resource arena identity exhausted")
}

/// Stable identity for one immutable canonical raster-image payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RasterImageResourceId(u64);

impl RasterImageResourceId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Versioned reference to one immutable raster-image resource.
///
/// This first B2 resource slice is append-only inside one arena, so live handles
/// remain at version zero. Arena provenance prevents equal slot values from a
/// cloned or unrelated scene store from aliasing one another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RasterImageResourceHandle {
    pub arena: u64,
    pub id: RasterImageResourceId,
    pub version: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct RasterImageResourceKey {
    width: u32,
    height: u32,
    rgba8: Arc<[u8]>,
}

/// Immutable renderer/backend-neutral canonical raster content.
///
/// File encodings such as PNG/JPEG/WebP and Python ndarray/PIL inputs are loader
/// concerns. They normalize to tightly packed row-major RGBA8 before entering this
/// retained arena, matching the pinned Manim model in which image behavior operates
/// on a normalized RGBA pixel array. Renderer texture residency remains derived and
/// disposable; it is not stored here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RasterImageResource {
    width: u32,
    height: u32,
    rgba8: Arc<[u8]>,
}

impl RasterImageResource {
    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba8(&self) -> &[u8] {
        self.rgba8.as_ref()
    }

    pub fn retained_bytes(&self) -> usize {
        size_of::<Self>().saturating_add(self.rgba8.len())
    }

    fn key(&self) -> RasterImageResourceKey {
        RasterImageResourceKey {
            width: self.width,
            height: self.height,
            rgba8: self.rgba8.clone(),
        }
    }
}

#[derive(Clone, Debug)]
struct RasterImageResourceEntry {
    version: u64,
    value: Arc<RasterImageResource>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RasterImageResourceStats {
    pub live_resources: usize,
    pub retained_bytes: usize,
    pub pixel_bytes: usize,
}

/// Content-deduplicating arena for immutable canonical raster images.
///
/// Equal intrinsic dimensions and RGBA8 pixels share one retained allocation.
/// This is resource identity only: semantic object identity, scene membership,
/// transforms, opacity, sampling policy and GPU texture residency remain owned by
/// their normal architecture layers.
#[derive(Debug)]
pub struct RasterImageResourceArena {
    namespace: u64,
    entries: Vec<RasterImageResourceEntry>,
    handles_by_content: HashMap<RasterImageResourceKey, RasterImageResourceHandle>,
    retained_bytes: usize,
    pixel_bytes: usize,
}

impl Default for RasterImageResourceArena {
    fn default() -> Self {
        Self {
            namespace: next_raster_image_resource_arena(),
            entries: Vec::new(),
            handles_by_content: HashMap::new(),
            retained_bytes: 0,
            pixel_bytes: 0,
        }
    }
}

// A cloned arena is an independent resource owner. Payload allocations remain
// shared through Arc, while every derived handle is rewritten to the clone's
// namespace so stale/foreign provenance cannot alias the clone.
impl Clone for RasterImageResourceArena {
    fn clone(&self) -> Self {
        let namespace = next_raster_image_resource_arena();
        let mut handles_by_content = self.handles_by_content.clone();
        for handle in handles_by_content.values_mut() {
            handle.arena = namespace;
        }
        Self {
            namespace,
            entries: self.entries.clone(),
            handles_by_content,
            retained_bytes: self.retained_bytes,
            pixel_bytes: self.pixel_bytes,
        }
    }
}

impl RasterImageResourceArena {
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern one already-normalized tightly packed RGBA8 image.
    ///
    /// Repeated identical visual content is allocation-free after lookup. Invalid
    /// dimensions/lengths fail before the arena changes.
    pub fn intern_rgba8(
        &mut self,
        width: u32,
        height: u32,
        rgba8: impl Into<Arc<[u8]>>,
    ) -> Result<RasterImageResourceHandle, RasterImageResourceError> {
        let expected = expected_rgba8_len(width, height)?;
        let rgba8 = rgba8.into();
        if rgba8.len() != expected {
            return Err(RasterImageResourceError::InvalidPixelLength {
                expected,
                actual: rgba8.len(),
            });
        }

        let resource = RasterImageResource {
            width,
            height,
            rgba8,
        };
        let key = resource.key();
        if let Some(handle) = self.handles_by_content.get(&key).copied() {
            return Ok(handle);
        }

        let id = RasterImageResourceId::new(
            u64::try_from(self.entries.len())
                .expect("Noon raster image resource ID space exhausted"),
        );
        let handle = RasterImageResourceHandle {
            arena: self.namespace,
            id,
            version: 0,
        };
        let resource = Arc::new(resource);
        self.retained_bytes = self
            .retained_bytes
            .saturating_add(resource.retained_bytes());
        self.pixel_bytes = self.pixel_bytes.saturating_add(resource.rgba8.len());
        self.entries.push(RasterImageResourceEntry {
            version: 0,
            value: resource,
        });
        self.handles_by_content.insert(key, handle);
        Ok(handle)
    }

    pub fn get(&self, handle: RasterImageResourceHandle) -> Option<&RasterImageResource> {
        if handle.arena != self.namespace {
            return None;
        }
        let entry = self.entries.get(handle.id.get() as usize)?;
        (entry.version == handle.version).then_some(entry.value.as_ref())
    }

    pub fn get_shared(
        &self,
        handle: RasterImageResourceHandle,
    ) -> Option<Arc<RasterImageResource>> {
        if handle.arena != self.namespace {
            return None;
        }
        let entry = self.entries.get(handle.id.get() as usize)?;
        (entry.version == handle.version).then(|| entry.value.clone())
    }

    pub const fn stats(&self) -> RasterImageResourceStats {
        RasterImageResourceStats {
            live_resources: self.entries.len(),
            retained_bytes: self.retained_bytes,
            pixel_bytes: self.pixel_bytes,
        }
    }

    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn expected_rgba8_len(width: u32, height: u32) -> Result<usize, RasterImageResourceError> {
    if width == 0 || height == 0 {
        return Err(RasterImageResourceError::InvalidDimensions { width, height });
    }
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(RasterImageResourceError::ImageTooLarge { width, height })?;
    usize::try_from(pixels)
        .map_err(|_| RasterImageResourceError::ImageTooLarge { width, height })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterImageResourceError {
    InvalidDimensions { width: u32, height: u32 },
    ImageTooLarge { width: u32, height: u32 },
    InvalidPixelLength { expected: usize, actual: usize },
}

impl std::fmt::Display for RasterImageResourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDimensions { width, height } => write!(
                formatter,
                "raster image dimensions must be non-zero, got {width}x{height}",
            ),
            Self::ImageTooLarge { width, height } => write!(
                formatter,
                "raster image dimensions {width}x{height} exceed addressable RGBA8 storage",
            ),
            Self::InvalidPixelLength { expected, actual } => write!(
                formatter,
                "raster RGBA8 payload has {actual} bytes, expected {expected}",
            ),
        }
    }
}

impl std::error::Error for RasterImageResourceError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(width: u32, height: u32, seed: u8) -> Arc<[u8]> {
        let len = expected_rgba8_len(width, height).unwrap();
        (0..len)
            .map(|index| seed.wrapping_add(index as u8))
            .collect::<Vec<_>>()
            .into()
    }

    #[test]
    fn repeated_identical_image_reuses_one_resource() {
        let mut arena = RasterImageResourceArena::new();
        let first = arena.intern_rgba8(320, 180, rgba(320, 180, 7)).unwrap();
        let second = arena.intern_rgba8(320, 180, rgba(320, 180, 7)).unwrap();

        assert_eq!(first, second);
        assert_eq!(arena.len(), 1);
        assert_eq!(arena.stats().pixel_bytes, 320 * 180 * 4);
        assert_eq!(arena.get(first).unwrap().rgba8().len(), 320 * 180 * 4);
    }

    #[test]
    fn intrinsic_dimensions_participate_in_content_identity() {
        let bytes = rgba(2, 2, 11);
        let mut arena = RasterImageResourceArena::new();
        let square = arena.intern_rgba8(2, 2, bytes.clone()).unwrap();
        let row = arena.intern_rgba8(4, 1, bytes).unwrap();

        assert_ne!(square, row);
        assert_eq!(arena.len(), 2);
    }

    #[test]
    fn invalid_resource_is_rejected_without_allocation() {
        let mut arena = RasterImageResourceArena::new();
        assert_eq!(
            arena.intern_rgba8(0, 10, Arc::<[u8]>::from([])),
            Err(RasterImageResourceError::InvalidDimensions {
                width: 0,
                height: 10,
            })
        );
        assert_eq!(
            arena.intern_rgba8(2, 2, Arc::<[u8]>::from([0; 15])),
            Err(RasterImageResourceError::InvalidPixelLength {
                expected: 16,
                actual: 15,
            })
        );
        assert!(arena.is_empty());
        assert_eq!(arena.stats(), RasterImageResourceStats::default());
    }

    #[test]
    fn foreign_arena_handle_is_rejected() {
        let mut first = RasterImageResourceArena::new();
        let mut second = RasterImageResourceArena::new();
        let a = first.intern_rgba8(4, 2, rgba(4, 2, 3)).unwrap();
        let b = second.intern_rgba8(4, 2, rgba(4, 2, 3)).unwrap();

        assert_eq!((a.id, a.version), (b.id, b.version));
        assert_ne!(a.arena, b.arena);
        assert!(first.get(b).is_none());
        assert!(second.get(a).is_none());
    }

    #[test]
    fn cloned_arena_is_renamespaced_but_shares_payload() {
        let mut source = RasterImageResourceArena::new();
        let source_handle = source.intern_rgba8(8, 4, rgba(8, 4, 19)).unwrap();
        let source_payload = source.get_shared(source_handle).unwrap();

        let cloned = source.clone();
        let cloned_handle = *cloned
            .handles_by_content
            .values()
            .next()
            .expect("cloned content index contains the image");
        let cloned_payload = cloned.get_shared(cloned_handle).unwrap();

        assert_ne!(source_handle.arena, cloned_handle.arena);
        assert!(cloned.get(source_handle).is_none());
        assert!(source.get(cloned_handle).is_none());
        assert!(Arc::ptr_eq(&source_payload, &cloned_payload));
    }
}
