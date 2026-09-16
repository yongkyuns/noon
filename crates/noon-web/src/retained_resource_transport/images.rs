//! Immutable image resources cross the genuine worker boundary once, independently
//! of compact object deltas. Installation uses unpublished arenas for rollback.
use super::*;
use crate::{TransportImageResourceHandle, TransportImageSampling};
use noon_core::{
    RasterImageContentRef, RasterImageResourceArena, RasterImageResourceHandle,
    RasterImageResourceLookup, RasterImageSampling, SemanticImageContent,
};

const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct TransportImageEntry {
    pub handle: TransportImageResourceHandle,
    width: u32,
    height: u32,
    #[serde(
        serialize_with = "serialize_pixels",
        deserialize_with = "deserialize_pixels"
    )]
    rgba8: Vec<u8>,
}

fn serialize_pixels<S: serde::Serializer>(pixels: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_bytes(pixels)
}
fn deserialize_pixels<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<u8>, D::Error> {
    struct Pixels;
    impl<'de> serde::de::Visitor<'de> for Pixels {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("bounded RGBA8 image bytes")
        }
        fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Vec<u8>, E> {
            if bytes.len() > MAX_IMAGE_BYTES {
                return Err(E::custom("image payload exceeds 64 MiB"));
            }
            Ok(bytes.to_vec())
        }
        fn visit_byte_buf<E: serde::de::Error>(self, bytes: Vec<u8>) -> Result<Vec<u8>, E> {
            if bytes.len() > MAX_IMAGE_BYTES {
                return Err(E::custom("image payload exceeds 64 MiB"));
            }
            Ok(bytes)
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Vec<u8>, A::Error> {
            use serde::de::Error;
            if sequence
                .size_hint()
                .is_some_and(|size| size > MAX_IMAGE_BYTES)
            {
                return Err(A::Error::custom("image payload exceeds 64 MiB"));
            }
            let mut bytes =
                Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(MAX_IMAGE_BYTES));
            while let Some(byte) = sequence.next_element()? {
                if bytes.len() == MAX_IMAGE_BYTES {
                    return Err(A::Error::custom("image payload exceeds 64 MiB"));
                }
                bytes.push(byte);
            }
            Ok(bytes)
        }
    }
    deserializer.deserialize_byte_buf(Pixels)
}

impl RetainedResourceBundle {
    pub fn with_images(
        mut self,
        handles: impl IntoIterator<Item = RasterImageResourceHandle>,
        images: &(impl RasterImageResourceLookup + ?Sized),
    ) -> Result<Self, RetainedResourceTransportError> {
        self.capture_images(handles, images)?;
        Ok(self)
    }

    pub(crate) fn capture_images(
        &mut self,
        handles: impl IntoIterator<Item = RasterImageResourceHandle>,
        images: &(impl RasterImageResourceLookup + ?Sized),
    ) -> Result<(), RetainedResourceTransportError> {
        let existing: HashSet<_> = self.images.iter().map(|image| image.handle).collect();
        let mut additions = Vec::new();
        for handle in handles.into_iter().collect::<BTreeSet<_>>() {
            let transport = TransportImageResourceHandle::from(handle);
            if existing.contains(&transport) {
                continue;
            }
            let image = images
                .get(handle)
                .ok_or(RetainedResourceTransportError::UnknownImage(transport))?;
            if image.rgba8().len() > MAX_IMAGE_BYTES {
                return Err(RetainedResourceTransportError::InvalidImage(
                    "worker image payload exceeds 64 MiB".into(),
                ));
            }
            additions.push(TransportImageEntry {
                handle: transport,
                width: image.width(),
                height: image.height(),
                rgba8: image.rgba8().to_vec(),
            });
        }
        // No partially captured payloads on an unknown resource/size failure.
        self.images.extend(additions);
        Ok(())
    }
    pub fn image_count(&self) -> usize {
        self.images.len()
    }
    pub fn image_bytes(&self) -> usize {
        self.images.iter().map(|image| image.rgba8.len()).sum()
    }
}

pub(super) type ImageHandles = HashMap<TransportImageResourceHandle, RasterImageContentRef>;

pub(super) fn install_images(
    entries: Vec<TransportImageEntry>,
) -> Result<(Arc<RasterImageResourceArena>, ImageHandles), RetainedResourceTransportError> {
    let mut arena = RasterImageResourceArena::new();
    let mut handles = HashMap::new();
    for entry in entries {
        if handles.contains_key(&entry.handle) {
            return Err(RetainedResourceTransportError::DuplicateImage(entry.handle));
        }
        if entry.handle.arena == 0
            || entry.handle.version != 0
            || entry.rgba8.len() > MAX_IMAGE_BYTES
        {
            return Err(RetainedResourceTransportError::InvalidImage(
                "invalid immutable image provenance or payload size".into(),
            ));
        }
        let handle = arena
            .intern_rgba8(entry.width, entry.height, entry.rgba8)
            .map_err(|error| RetainedResourceTransportError::InvalidImage(error.to_string()))?;
        let content = RasterImageContentRef::from_resource(
            SemanticImageContent::new(handle),
            arena.get(handle).expect("admitted image"),
        );
        handles.insert(entry.handle, content);
    }
    Ok((Arc::new(arena), handles))
}

impl InstalledRetainedResources {
    pub fn images(&self) -> &dyn RasterImageResourceLookup {
        self
    }
    pub fn image_count(&self) -> usize {
        self.images.len() + self.additions.iter().map(Self::image_count).sum::<usize>()
    }
    pub fn resolve_image_handle(
        &self,
        transport: TransportImageResourceHandle,
        sampling: TransportImageSampling,
    ) -> Option<RasterImageContentRef> {
        self.image_handles
            .get(&transport)
            .map(|content| content.with_sampling(RasterImageSampling::from(sampling)))
    }
    pub(crate) fn image_handle_remap(&self) -> ImageHandles {
        self.image_handles.clone()
    }
}
impl RasterImageResourceLookup for InstalledRetainedResources {
    fn get(&self, handle: RasterImageResourceHandle) -> Option<&noon_core::RasterImageResource> {
        self.images.get(handle).or_else(|| {
            self.image_layers
                .get(&handle.arena)
                .and_then(|&layer| self.additions.get(layer))
                .and_then(|resources| resources.images.get(handle))
        })
    }
}
impl PreparedRetainedResourceAdditions {
    pub(crate) fn image_handle_remap(&self) -> ImageHandles {
        self.installed.image_handle_remap()
    }
}
