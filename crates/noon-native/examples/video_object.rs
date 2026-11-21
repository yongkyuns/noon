//! A local MP4 as a regular scene object with seekable audio and picture.

use std::sync::Arc;

use noon::{AnimationOptions, NativeVideoAsset, Scene};
use noon_core::{SemanticAnimationCompositionKind, SemanticAnimationIntent};
use noon_native::NativeViewportConfig;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::args_os()
        .nth(1)
        .ok_or("usage: video_object INPUT.mp4")?;
    let asset = Arc::new(NativeVideoAsset::open(input)?);
    let duration = asset.frame_index().duration();
    let mut scene = Scene::new();
    let video = scene.video(asset, 0.0, 0.0..duration, 6.0)?;
    let mut target = video.copy_handle()?;
    target.shift(1.5, 0.0)?;
    let motion =
        scene.declare_transform_to(&video, &target, AnimationOptions::new().run_time(duration))?;
    let root = scene.declare_animation(
        SemanticAnimationIntent::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            children: vec![motion.node_id()],
        },
        AnimationOptions::new(),
    )?;
    noon_native::run_with_config(
        scene.into_execution_with_animation_root(&root)?,
        NativeViewportConfig {
            title: "Noon video object".into(),
            playback_controls: true,
            ..Default::default()
        },
    )?;
    Ok(())
}
