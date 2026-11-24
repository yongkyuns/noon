use super::*;
use crate::{TransportImageResourceHandle, TransportImageSampling};
use noon_core::{
    RasterImageContentRef, RasterImageResourceArena, RasterImageResourceHandle,
    RasterImageResourceLookup, RasterImageSampling, SemanticImageContent, VideoRasterResource,
};
const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct TransportImageEntry {
    pub handle: TransportImageResourceHandle,
    width: u32,
    height: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    video: Option<TransportVideo>,
    #[serde(
        serialize_with = "serialize_pixels",
        deserialize_with = "deserialize_pixels"
    )]
    rgba8: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportVideo {
    asset_id: u32,
    scene_start: f64,
    source_start: f64,
    source_end: f64,
    duration: f64,
    timestamps: Vec<f64>,
}

impl TransportVideo {
    fn capture(video: &VideoRasterResource) -> Result<Self, RetainedResourceTransportError> {
        let source = video.source();
        let asset_id = source.external_asset_id().ok_or_else(|| {
            RetainedResourceTransportError::InvalidImage(
                "video source does not have a browser asset identity".into(),
            )
        })?;
        let timing = video.timing();
        let trim = timing.source_range();
        Ok(Self {
            asset_id,
            scene_start: timing.scene_start(),
            source_start: trim.start,
            source_end: trim.end,
            duration: source.frame_index().duration(),
            timestamps: source.frame_index().timestamps().to_vec(),
        })
    }

    fn install(
        self,
        width: u32,
        height: u32,
    ) -> Result<VideoRasterResource, RetainedResourceTransportError> {
        let source = crate::video_asset::BrowserVideoSource::new(
            self.asset_id,
            width,
            height,
            self.timestamps,
            self.duration,
        )
        .map_err(RetainedResourceTransportError::InvalidImage)?;
        VideoRasterResource::new(
            std::sync::Arc::new(source),
            self.scene_start,
            self.source_start..self.source_end,
        )
        .map_err(|error| RetainedResourceTransportError::InvalidImage(error.to_string()))
    }
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

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("bounded RGBA8 image bytes")
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
        let existing: HashSet<_> = self.images.iter().map(|x| x.handle).collect();
        let mut additions = Vec::new();
        for handle in handles.into_iter().collect::<BTreeSet<_>>() {
            let transport = handle.into();
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
                video: image.video().map(TransportVideo::capture).transpose()?,
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
        self.images.iter().map(|x| x.rgba8.len()).sum()
    }
}
pub(super) type ImageHandles = HashMap<TransportImageResourceHandle, RasterImageContentRef>;

pub(super) type VideoHandles = HashSet<TransportImageResourceHandle>;

pub(super) fn install_images(
    entries: Vec<TransportImageEntry>,
) -> Result<(RasterImageResourceArena, ImageHandles, VideoHandles), RetainedResourceTransportError>
{
    let mut arena = RasterImageResourceArena::new();
    let mut handles = HashMap::new();
    let mut videos = HashSet::new();
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
        let handle = if let Some(video) = entry.video {
            if !entry.rgba8.is_empty() {
                return Err(RetainedResourceTransportError::InvalidImage(
                    "a video raster cannot include static image pixels".into(),
                ));
            }
            arena.intern_video(video.install(entry.width, entry.height)?)
        } else {
            arena.intern_rgba8(entry.width, entry.height, entry.rgba8)
        }
        .map_err(|e| RetainedResourceTransportError::InvalidImage(e.to_string()))?;
        let content = RasterImageContentRef::from_resource(
            SemanticImageContent::new(handle),
            arena.get(handle).expect("admitted image"),
        );
        if arena
            .get(handle)
            .and_then(noon_core::RasterImageResource::video)
            .is_some()
        {
            videos.insert(entry.handle);
        }
        handles.insert(entry.handle, content);
    }
    Ok((arena, handles, videos))
}
impl InstalledRetainedResources {
    pub fn images(&self) -> &dyn RasterImageResourceLookup {
        self
    }
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn video_rasters(
        &self,
    ) -> impl Iterator<
        Item = (
            TransportImageResourceHandle,
            RasterImageResourceHandle,
            &VideoRasterResource,
        ),
    > {
        self.video_handles.iter().filter_map(|&origin| {
            let handle = self.image_handles.get(&origin)?.resource();
            RasterImageResourceLookup::get(self, handle)
                .and_then(|raster| raster.video())
                .map(|video| (origin, handle, video))
        })
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
            .map(|x| x.with_sampling(RasterImageSampling::from(sampling)))
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
                .and_then(|&i| self.additions.get(i))
                .and_then(|x| x.images.get(handle))
        })
    }
}

impl PreparedRetainedResourceAdditions {
    pub(crate) fn image_handle_remap(&self) -> ImageHandles {
        self.installed.image_handle_remap()
    }
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;

    #[test]
    fn browser_video_descriptor_preserves_media_time_through_addition_and_retirement() {
        let source =
            crate::video_asset::BrowserVideoSource::new(3, 96, 54, vec![0.0, 0.1, 0.35], 0.5)
                .unwrap();
        let mut arena = RasterImageResourceArena::new();
        let image = arena
            .intern_video(VideoRasterResource::new(Arc::new(source), 1.0, 0.05..0.5).unwrap())
            .unwrap();
        let bundle = RetainedResourceBundle::empty()
            .with_images([image], &arena)
            .unwrap();
        let mut installed = RetainedResourceBundle::empty().install().unwrap();
        let addition = installed.prepare_additions(bundle).unwrap();
        installed.commit_additions(addition);
        let (origin, _, video) = installed.video_rasters().next().unwrap();
        assert_eq!(origin, image.into());
        assert_eq!(video.source().external_asset_id(), Some(3));
        assert_eq!(
            video.timing().frame_at(video.source().frame_index(), 1.29),
            Some(1)
        );
        assert_eq!(
            video.timing().frame_at(video.source().frame_index(), 1.31),
            Some(2)
        );
        assert_eq!(
            video.timing().frame_at(video.source().frame_index(), 1.46),
            None
        );
        installed.retire(&RetainedResourceRetirements {
            images: vec![image.into()],
            ..Default::default()
        });
        assert_eq!(installed.video_rasters().count(), 0);
    }

    #[test]
    fn diagnostic_resource_handles_track_append_retirement_and_exact_ids() {
        let mut arena = RasterImageResourceArena::new();
        let first = arena.intern_rgba8(1, 1, vec![1, 2, 3, 4]).unwrap();
        let second = arena.intern_rgba8(1, 1, vec![5, 6, 7, 8]).unwrap();
        let base = RetainedResourceBundle::empty()
            .with_images([first], &arena)
            .unwrap();
        let mut inventory = base.inventory();
        let mut installed = base.install().unwrap();
        let mut addition = RetainedResourceBundle::empty()
            .with_images([second], &arena)
            .unwrap();
        let large_handle = TransportImageResourceHandle {
            arena: 17,
            id: 9_007_199_254_740_993,
            version: 0,
        };
        addition.images.push(TransportImageEntry {
            handle: large_handle,
            width: 1,
            height: 1,
            video: None,
            rgba8: vec![9, 10, 11, 12],
        });
        addition.retain_additions(&mut inventory);
        let prepared = installed.prepare_additions(addition).unwrap();
        installed.commit_additions(prepared);

        let snapshot = installed.diagnostic_resource_handles(1);
        assert_eq!(snapshot["total"], 3);
        assert_eq!(snapshot["omitted"], 2);
        assert_eq!(snapshot["records"][0]["kind"], "image");
        let assert_handles =
            |snapshot: serde_json::Value, expected: &[TransportImageResourceHandle]| {
                assert_eq!(snapshot["total"], expected.len());
                assert_eq!(snapshot["omitted"], 0);
                let records = snapshot["records"].as_array().unwrap();
                assert_eq!(records.len(), expected.len());
                for handle in expected {
                    let record = serde_json::json!({"kind": "image", "handle": {
                        "arena": handle.arena.to_string(), "id": handle.id.to_string(),
                        "version": handle.version.to_string(),
                    }});
                    assert!(records.contains(&record));
                }
            };
        assert_handles(
            installed.diagnostic_resource_handles(3),
            &[first.into(), second.into(), large_handle],
        );
        installed.retire_image(first.into());
        assert_handles(
            installed.diagnostic_resource_handles(3),
            &[second.into(), large_handle],
        );
    }
}
