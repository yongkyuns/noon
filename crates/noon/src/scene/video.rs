use std::{ops::Range, sync::Arc};

use super::*;
use crate::{ImageMobjectOptions, UnsupportedAuthoringOperation};
use noon_core::{
    AudioClipDefinition, AudioResource, RasterImageResource, VideoFrameSource, VideoRasterResource,
};

#[cfg(all(feature = "video-decode", not(target_arch = "wasm32")))]
impl Scene {
    /// Place a prepared MP4 in this scene and declare its soundtrack atomically.
    /// Timing is absolute scene time and does not advance the authoring cursor.
    /// The caller can animate the returned image quad through ordinary Scene APIs.
    pub fn video(
        &mut self,
        asset: Arc<crate::NativeVideoAsset>,
        start_time: f64,
        trim: Range<f64>,
        height: f64,
    ) -> Result<Mobject, AuthoringError> {
        place_video(
            self,
            asset.clone(),
            asset.soundtrack(),
            start_time,
            trim,
            height,
        )
    }
}

/// Place a prepared clip from a native or browser media host.
pub fn place_video(
    scene: &mut Scene,
    source: Arc<dyn VideoFrameSource>,
    soundtrack: Option<(&AudioResource, f64)>,
    start_time: f64,
    trim: Range<f64>,
    height: f64,
) -> Result<Mobject, AuthoringError> {
    if scene.running_execution().is_some() {
        return Err(AuthoringError::Unsupported(
            UnsupportedAuthoringOperation::VideoRequiresSetup,
        ));
    }
    let video =
        VideoRasterResource::new(source, start_time, trim).map_err(AuthoringError::VideoTiming)?;
    let audio = match soundtrack {
        Some((pcm, offset)) => video
            .timing()
            .audio_options(pcm, offset)
            .map_err(AuthoringError::VideoTiming)?
            .map(|options| (pcm.clone(), options)),
        None => None,
    };
    let resource = RasterImageResource::from_video(video)?;
    let mut display = ImageMobjectOptions::from_resource(resource.clone());
    display.set_height(height)?;
    display.set_sampling(noon_core::RasterImageSampling::Linear);
    let make_state = display.state_builder()?;
    let root = scene.root;
    let node = scene.with_semantic_publication(|store, publish| {
        store.with_raster_image(resource, |store, handle| {
            let mut transaction = SemanticMutationTransaction::new();
            let token = transaction.create_node(SemanticNodeCreation::object(make_state(handle)));
            transaction.add_member(root, token);
            let result = if let Some((pcm, options)) = audio {
                store.with_audio_resource(pcm, |store, handle| {
                    let pcm = store.audio_resources().get(handle).expect("admitted PCM");
                    transaction
                        .create_audio_clip(root, AudioClipDefinition::new(handle, pcm, options)?);
                    publish(store, transaction)
                })?
            } else {
                publish(store, transaction)?
            };
            result
                .resolve(token)
                .ok_or(AuthoringError::UnresolvedCreatedNode(token))
        })
    })?;
    Mobject::from_node(Rc::clone(&scene.store), node)
}
