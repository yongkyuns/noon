//! A local MP4 as a regular scene object with seekable audio and picture.

use std::sync::Arc;

use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
use noon::{NativeVideoAsset, Scene};
use noon_core::{RateFunction, SemanticVec3};
use noon_export::output::{export_scene_file, OutputOptions};
use noon_export::CaptureOptions;
use noon_native::NativeViewportConfig;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let usage = "usage: video_object INPUT.mp4 [OUTPUT.mp4]";
    let input = args.next().ok_or(usage)?;
    let output = args.next();
    if args.next().is_some() {
        return Err(usage.into());
    }
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
    let mut execution = scene.into_execution()?;
    if let Some(output) = output {
        let movie = export_scene_file(
            &mut execution,
            ExportFrameOptions {
                frame_rate: FrameRate::new(30_000, 1_001)?,
                start_frame: 0,
                stop: ExportStop::SourceEnd,
                max_frames: 2_000,
                max_transitions_per_sample: 64,
                final_hold_seconds: 0.0,
            },
            CaptureOptions::new(640, 360),
            OutputOptions::mp4(output),
        )?;
        println!("{}", movie.path.display());
        return Ok(());
    }
    noon_native::run_with_config(
        execution,
        NativeViewportConfig {
            title: "Noon video object".into(),
            playback_controls: true,
            ..Default::default()
        },
    )?;
    Ok(())
}
