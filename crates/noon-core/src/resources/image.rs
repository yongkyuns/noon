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

/// Encodings accepted by the Phase B raster-image resource boundary.
///
/// Decoding and encoded-header validation happen before or during resource
/// installation. The retained resource stores the original immutable payload so
/// browser/native preparation can derive backend-local decoded/texture state
/// without making that state semantic authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RasterImageEncoding {
    Png,
    Jpeg,
    Webp,
}

/// Stable identity for one immutable encoded raster-image payload.
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
    encoding: RasterImageEncoding,
    width: u32,
    height: u32,
    data: Arc<[u8]>,
}

/// Immutable renderer/backend-neutral encoded image resource.
///
/// `width`/`height` are intrinsic dimensions established by the decoder/resource
/// preparation boundary. This arena intentionally does not parse image formats;
/// format-specific parsing belongs to the #79 loader/decoder follow-up and must
/// occur off the frame path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RasterImageResource {
    encoding: RasterImageEncoding,
    width: u32,
    height: u32,
    data: Arc<[u8]>,
}

impl RasterImageResource {
    pub const fn encoding(&self) -> RasterImageEncoding {
        self.encoding
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn data(&self) -> &[u8] {
        self.data.as_ref()
    }

    pub fn retained_bytes(&self) -> usize {
        size_of::<Self>().saturating_add(self.data.len())
    }

    fn key(&self) -> RasterImageResourceKey {
        RasterImageResourceKey {
            encoding: self.encoding,
            width: self.width,
            height: self.height,
            data: self.data.clone(),
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
    pub encoded_bytes: usize,
}

/// Content-deduplicating arena for immutable encoded raster images.
///
/// Equal encoding, intrinsic dimensions and encoded bytes share one retained
/// allocation. This is resource identity only: semantic object identity, scene
/// membership, transforms, opacity and GPU texture residency remain owned by
/// their normal architecture layers.
#[derive(Debug)]
pub struct RasterImageResourceArena {
    namespace: u64,
    entries: Vec<RasterImageResourceEntry>,
    handles_by_content: HashMap<RasterImageResourceKey, RasterImageResourceHandle>,
    retained_bytes: usize,
    encoded_bytes: usize,
}

impl Default for RasterImageResourceArena {
    fn default() -> Self {
        Self {
            namespace: next_raster_image_resource_arena(),
            entries: Vec::new(),
            handles_by_content: HashMap::new(),
            retained_bytes: 0,
            encoded_bytes: 0,
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
            encoded_bytes: self.encoded_bytes,
        }
    }
}

impl RasterImageResourceArena {
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern one already-prepared encoded image.
    ///
    /// The caller supplies dimensions verified by its decoder/resource-preparation
    /// boundary. Repeated identical content is allocation-free after lookup.
    pub fn intern_encoded(
        &mut self,
        encoding: RasterImageEncoding,
        width: u32,
        height: u32,
        data: impl Into<Arc<[u8]>>,
    ) -> Result<RasterImageResourceHandle, RasterImageResourceError> {
        if width == 0 || height == 0 {
            return Err(RasterImageResourceError::InvalidDimensions { width, height });
        }
        let data = data.into();
        if data.is_empty() {
            return Err(RasterImageResourceError::EmptyPayload);
        }

        let resource = RasterImageResource {
            encoding,
            width,
            height,
            data,
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
        self.encoded_bytes = self.encoded_bytes.saturating_add(resource.data.len());
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
            encoded_bytes: self.encoded_bytes,
        }
    }

    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterImageResourceError {
    EmptyPayload,
    InvalidDimensions { width: u32, height: u32 },
}

impl std::fmt::Display for RasterImageResourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPayload => formatter.write_str("raster image payload is empty"),
            Self::InvalidDimensions { width, height } => write!(
                formatter,
                "raster image dimensions must be non-zero, got {width}x{height}",
            ),
        }
    }
}

impl std::error::Error for RasterImageResourceError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes() -> Arc<[u8]> {
        Arc::from([0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a])
    }

    #[test]
    fn repeated_identical_image_reuses_one_resource() {
        let mut arena = RasterImageResourceArena::new();
        let first = arena
            .intern_encoded(RasterImageEncoding::Png, 320, 180, png_bytes())
            .unwrap();
        let second = arena
            .intern_encoded(RasterImageEncoding::Png, 320, 180, png_bytes())
            .unwrap();

        assert_eq!(first, second);
        assert_eq!(arena.len(), 1);
        assert_eq!(arena.stats().encoded_bytes, png_bytes().len());
    }

    #[test]
    fn metadata_participates_in_content_identity() {
        let mut arena = RasterImageResourceArena::new();
        let png = arena
            .intern_encoded(RasterImageEncoding::Png, 320, 180, png_bytes())
            .unwrap();
        let jpeg = arena
            .intern_encoded(RasterImageEncoding::Jpeg, 320, 180, png_bytes())
            .unwrap();
        let resized = arena
            .intern_encoded(RasterImageEncoding::Png, 640, 360, png_bytes())
            .unwrap();

        assert_ne!(png, jpeg);
        assert_ne!(png, resized);
        assert_eq!(arena.len(), 3);
    }

    #[test]
    fn invalid_resource_is_rejected_without_allocation() {
        let mut arena = RasterImageResourceArena::new();
        assert_eq!(
            arena.intern_encoded(RasterImageEncoding::Png, 0, 10, png_bytes()),
            Err(RasterImageResourceError::InvalidDimensions {
                width: 0,
                height: 10,
            })
        );
        assert_eq!(
            arena.intern_encoded(
                RasterImageEncoding::Webp,
                10,
                10,
                Arc::<[u8]>::from([]),
            ),
            Err(RasterImageResourceError::EmptyPayload)
        );
        assert!(arena.is_empty());
        assert_eq!(arena.stats(), RasterImageResourceStats::default());
    }

    #[test]
    fn foreign_arena_handle_is_rejected() {
        let mut first = RasterImageResourceArena::new();
        let mut second = RasterImageResourceArena::new();
        let a = first
            .intern_encoded(RasterImageEncoding::Png, 32, 18, png_bytes())
            .unwrap();
        let b = second
            .intern_encoded(RasterImageEncoding::Png, 32, 18, png_bytes())
            .unwrap();

        assert_eq!((a.id, a.version), (b.id, b.version));
        assert_ne!(a.arena, b.arena);
        assert!(first.get(b).is_none());
        assert!(second.get(a).is_none());
    }

    #[test]
    fn cloned_arena_is_renamespaced_but_shares_payload() {
        let mut source = RasterImageResourceArena::new();
        let source_handle = source
            .intern_encoded(RasterImageEncoding::Png, 320, 180, png_bytes())
            .unwrap();
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
