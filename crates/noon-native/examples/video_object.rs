//! A local MP4 as a regular scene object with seekable audio and picture.

use std::sync::Arc;

use noon::{NativeVideoAsset, Scene};
use noon_core::{RateFunction, SemanticVec3};
use noon_native::NativeViewportConfig;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::args_os()
        .nth(1)
        .ok_or("usage: video_object INPUT.mp4")?;
    let asset = Arc::new(NativeVideoAsset::open(input)?);
    let duration = asset.frame_index().duration();
    let mut scene = Scene::new();
    let video = scene.video(asset, 0.0, 0.0..duration, 6.0)?;
    let translation = scene.value_tracker(0.0)?;
    let position = scene.position_from_tracker(
        &translation,
        SemanticVec3::new(1.0, 0.0, 0.0),
        SemanticVec3::new(0.0, 0.0, 0.0),
    )?;
    scene.bind_position(&video, &position)?;
    scene
        .play_value(&translation, 1.5)
        .rate_func(RateFunction::Linear)
        .run_time(2.0)?;
    noon_native::run_with_config(
        scene.into_execution()?,
        NativeViewportConfig {
            title: "Noon video object".into(),
            playback_controls: true,
            ..Default::default()
        },
    )?;
    Ok(())
}
