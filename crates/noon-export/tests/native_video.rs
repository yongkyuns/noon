#![cfg(all(feature = "video-decode", not(target_arch = "wasm32")))]

use std::{fs, path::Path, process::Command, sync::Arc, time::Duration};

use noon::{
    integration::{ExportFrameOptions, ExportStop, FrameRate},
    NativeVideoAsset, NativeVideoCache, Scene,
};
use noon_export::{
    output::{export_scene_file, OutputOptions},
    CaptureOptions, SessionCapture,
};

#[test]
#[ignore = "requires the native FFmpeg media tools"]
fn native_video_seek_and_export_preserve_pts_and_soundtrack() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vfr.mp4");
    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-nostdin",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=96x54:rate=12:duration=2",
            "-itsoffset",
            "0.25",
            "-f",
            "lavfi",
            "-i",
            "aevalsrc=if(lt(t\\,0.025)\\,0.5*sin(2*PI*1800*t)\\,0):s=48000:d=1.7",
            "-vf",
            "drawbox=x=0:y=0:w=iw:h=ih:color=white:t=fill:enable=eq(n\\,3),select=not(eq(mod(n\\,5)\\,2))",
            "-fps_mode",
            "vfr",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-g",
            "6",
            "-bf",
            "2",
            "-sc_threshold",
            "0",
            "-c:a",
            "aac",
        ])
        .arg(&path)
        .output()
        .expect("run FFmpeg fixture generator");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let asset = NativeVideoAsset::open(&path).unwrap();
    assert_eq!(asset.dimensions(), (96, 54));
    let timestamps = asset.frame_index().timestamps();
    assert_eq!(timestamps.len(), 19);
    assert!(timestamps[2] - timestamps[1] > timestamps[1] - timestamps[0]);
    fs::remove_file(path).unwrap();

    let mut sequential = asset.decode_from(0).unwrap();
    let reference: Vec<_> = std::iter::from_fn(|| sequential.read_frame().transpose())
        .collect::<std::io::Result<_>>()
        .unwrap();
    assert_eq!(reference.len(), timestamps.len());
    drop(sequential);
    for index in [14, 7, 16, 0] {
        let mut reader = asset.decode_from(index).unwrap();
        assert_eq!(reader.read_frame().unwrap().as_ref(), reference.get(index));
        assert_eq!(
            reader.read_frame().unwrap().as_ref(),
            reference.get(index + 1)
        );
    }

    let (pcm, audio_start) = asset.soundtrack().unwrap();
    let first_click = pcm
        .samples()
        .as_chunks::<2>()
        .0
        .iter()
        .position(|frame| frame[0].abs() > 0.2)
        .unwrap();
    let click_time = audio_start + first_click as f64 / f64::from(pcm.sample_rate());
    assert!(
        (0.247..0.253).contains(&click_time),
        "first audible peak at {click_time}s"
    );

    let asset = Arc::new(asset);
    let cache = NativeVideoCache::new(asset.clone(), || {}).unwrap();
    let deadline = Duration::from_secs(3);
    for index in [0, 1, 2, 16, 7, 0] {
        cache.try_frame(index + 1).unwrap();
        let pixels = cache.wait_frame(index, deadline).unwrap();
        assert_eq!(pixels.as_slice(), reference[index]);
        assert!(Arc::ptr_eq(
            &pixels,
            &cache.try_frame(index).unwrap().unwrap()
        ));
    }

    drop(cache);

    let scene_end = 1.225;
    let mut scene = Scene::new();
    let video = scene.video(asset, 0.1, 0.125..1.25, 8.0).unwrap();
    scene.set_opacity(&video, 0.8).unwrap();
    let mut scene = scene.into_execution().unwrap();
    let session = scene.session();
    assert_eq!(session.finite_playback_end_time(), Some(scene_end));
    assert_eq!(session.video_rasters().len(), 1);
    assert!(session.has_audio());

    let capture_options = CaptureOptions::new(128, 72);
    let mut capture = SessionCapture::new(scene.session(), capture_options.clone()).unwrap();
    scene.seek(0.4).unwrap();
    let late = capture.capture(scene.session_mut()).unwrap().rgba.to_vec();
    assert!(late.iter().any(|&byte| (1..220).contains(&byte)));
    scene.seek(0.2).unwrap();
    let early = capture.capture(scene.session_mut()).unwrap().rgba.to_vec();
    assert_ne!(early, late);
    scene.seek(0.4).unwrap();
    assert_eq!(capture.capture(scene.session_mut()).unwrap().rgba, late);
    let paused = capture.capture(scene.session_mut()).unwrap();
    assert_eq!(paused.receipt.work.retained_upload_bytes, 0);
    assert_eq!(paused.rgba, late);
    scene.seek(0.0).unwrap();
    assert!(capture
        .capture(scene.session_mut())
        .unwrap()
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .all(|pixel| *pixel == [0, 0, 0, 255]));

    drop(capture);
    let fps = FrameRate::new(30_000, 1_001).unwrap();
    for start_frame in [0, 4] {
        let path = directory.path().join(format!("export-{start_frame}.mp4"));
        let export = export_scene_file(
            &mut scene,
            ExportFrameOptions {
                frame_rate: fps,
                start_frame,
                stop: ExportStop::SourceEnd,
                max_frames: 64,
                max_transitions_per_sample: 32,
                final_hold_seconds: 0.0,
            },
            capture_options.clone(),
            OutputOptions::mp4(&path),
        )
        .unwrap();
        assert_eq!(export.capture.sampling.frames, 37 - start_frame);

        let samples = decode_output(&path, &["-vn", "-ac", "1", "-ar", "48000", "-f", "f32le"]);
        let first_click = samples
            .as_chunks::<4>()
            .0
            .iter()
            .position(|bytes| f32::from_le_bytes(*bytes).abs() > 0.12)
            .unwrap();
        let actual = first_click as f64 / 48_000.0;
        let expected = 0.225 - start_frame as f64 * (1001.0 / 30_000.0);
        assert!(
            (actual - expected).abs() < 0.012,
            "click {actual}s, expected {expected}s"
        );

        let video = decode_output(&path, &["-an", "-pix_fmt", "rgb24", "-f", "rawvideo"]);
        let (frames, remainder) = video.as_chunks::<{ 128 * 72 * 3 }>();
        assert!(remainder.is_empty());
        let mut frames = frames.iter();
        let first = frames.next().unwrap();
        assert_eq!(first.iter().any(|&pixel| pixel > 25), start_frame != 0);
        let flash = frames
            .position(|frame| frame.iter().all(|&pixel| pixel > 170))
            .expect("source flash must appear in the rendered clip")
            + 1;
        let visual_time = flash as f64 * (1001.0 / 30_000.0);
        assert!(
            (visual_time - actual).abs() < 1001.0 / 30_000.0,
            "flash at {visual_time}s and audio click at {actual}s"
        );
    }
}

fn decode_output(path: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(args)
        .arg("pipe:1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
