#![cfg(all(feature = "video-decode", not(target_arch = "wasm32")))]

use std::{fs, path::Path, process::Command, sync::Arc, time::Duration};

use noon::{
    integration::{ExportFrameOptions, ExportStop, FrameRate},
    NativeVideoAsset, NativeVideoCache, Scene,
};
use noon_core::Vec2;
use noon_export::{
    output::{export_scene_file, OutputOptions},
    CaptureOptions, SessionCapture,
};

#[test]
#[ignore = "requires the native FFmpeg media tools"]
fn native_video_seek_and_export_preserve_pts_and_soundtrack() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vfr.mp4");
    generate_fixture(&path);

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
    scene.scale(&video, 0.5, 0.5).unwrap();
    scene.shift(&video, Vec2::new(1.0, 0.0)).unwrap();
    scene.set_opacity(&video, 0.8).unwrap();
    let mut scene = scene.into_execution().unwrap();
    let session = scene.session();
    assert_eq!(session.finite_playback_end_time(), Some(scene_end));
    assert_eq!(session.video_rasters().len(), 1);
    assert!(session.has_audio());

    let capture_options = CaptureOptions::new(128, 72);
    let mut capture = SessionCapture::new(scene.session(), capture_options.clone()).unwrap();
    scene.seek(0.1).unwrap();
    let entry = capture.capture(scene.session_mut()).unwrap().rgba.to_vec();
    assert!(entry
        .as_chunks::<4>()
        .0
        .iter()
        .any(|pixel| pixel[..3] != [0, 0, 0]));
    scene.seek(0.4).unwrap();
    let late = capture.capture(scene.session_mut()).unwrap().rgba.to_vec();
    scene.seek(0.2).unwrap();
    let early = capture.capture(scene.session_mut()).unwrap().rgba.to_vec();
    assert_ne!(early, late);
    scene.session_mut().advance_to(0.4).unwrap();
    assert_eq!(capture.capture(scene.session_mut()).unwrap().rgba, late);
    let paused = capture.capture(scene.session_mut()).unwrap();
    assert_eq!(paused.receipt.work.retained_upload_bytes, 0);
    assert_eq!(paused.rgba, late);
    scene.seek(0.26).unwrap();
    let flash = capture.capture(scene.session_mut()).unwrap();
    let pixel = |x: usize, y: usize| &flash.rgba[(y * 128 + x) * 4..][..4];
    assert!(pixel(64, 36)[..3]
        .iter()
        .all(|value| (195..=210).contains(value)));
    assert_eq!(pixel(36, 36), &[0, 0, 0, 255]);
    assert_eq!(pixel(64, 15), &[0, 0, 0, 255]);
    for time in [scene_end, 0.0] {
        scene.seek(time).unwrap();
        assert!(capture
            .capture(scene.session_mut())
            .unwrap()
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [0, 0, 0, 255]));
    }
    scene.session_mut().advance_to(0.1).unwrap();
    assert_eq!(capture.capture(scene.session_mut()).unwrap().rgba, entry);
    scene.seek(0.0).unwrap();

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

        assert_synced_movie(&path, start_frame, 37);
    }
}

#[cfg(feature = "prepared-prefix")]
#[test]
#[ignore = "requires the native renderer and FFmpeg media tools"]
fn completed_play_wait_exports_a_synced_movie() {
    use noon::{AnimationOptions, RateFunction};
    use noon_export::output::export_completed_prefix_file;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vfr.mp4");
    generate_fixture(&path);
    let mut scene = Scene::new();
    let video = scene
        .video(
            Arc::new(NativeVideoAsset::open(path).unwrap()),
            0.1,
            0.125..1.25,
            8.0,
        )
        .unwrap();
    scene.scale(&video, 0.5, 0.5).unwrap();
    scene.shift(&video, Vec2::new(1.0, 0.0)).unwrap();
    scene.set_opacity(&video, 0.8).unwrap();
    let mut marker = scene.circle(0.3).unwrap();
    marker.set_translation(-3.0, 0.0).unwrap();
    marker.set_fill(1.0, 0.0, 0.0, 1.0).unwrap();
    scene.add(&marker).unwrap();
    let mut target = scene.circle(0.3).unwrap();
    target.set_translation(-3.0, 2.0).unwrap();
    target.set_fill(1.0, 0.0, 0.0, 1.0).unwrap();

    let mut source = scene.into_execution().unwrap();
    source
        .session_mut()
        .begin_replay_retention(Default::default())
        .unwrap();
    let play = source
        .live()
        .declare_and_activate_transform_to(
            &marker,
            &target,
            AnimationOptions::new()
                .run_time(0.4)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    source.session_mut().advance_to(play.end_time()).unwrap();
    source.live().complete_segment(play).unwrap();
    let (mut publisher, mut reader) = source.session().create_prepared_prefix(play).unwrap();
    let wait = source.live().wait_segment(0.925).unwrap();
    source.session_mut().advance_to(wait.end_time()).unwrap();
    source.live().complete_segment(wait).unwrap();
    source.session_mut().seal_replay().unwrap();
    let terminal = source
        .session()
        .finish_prepared_prefix(wait, &mut publisher)
        .unwrap();
    reader.reserve_append_capacity(&terminal).unwrap();
    reader.append(terminal).unwrap();
    let runtime = reader.runtime_identity();
    drop(source);

    for start_frame in [0, 4] {
        reader.seek(0.0).unwrap();
        let path = directory.path().join(format!("history-{start_frame}.mp4"));
        let export = export_completed_prefix_file(
            &mut reader,
            ExportFrameOptions {
                frame_rate: FrameRate::new(30_000, 1_001).unwrap(),
                start_frame,
                stop: ExportStop::SourceEnd,
                max_frames: 64,
                max_transitions_per_sample: 32,
                final_hold_seconds: 0.0,
            },
            CaptureOptions::new(128, 72),
            OutputOptions::mp4(&path),
        )
        .unwrap();
        assert_eq!(reader.runtime_identity(), runtime);
        assert_eq!(export.capture.sampling.frames, 40 - start_frame);
        assert_eq!(export.capture.sampling.source_end, Some(wait.end_time()));
        assert_synced_movie(&path, start_frame, 40);
    }
}

fn generate_fixture(path: &Path) {
    let output = Command::new("ffmpeg")
        .args(include_str!("fixtures/video-pts-and-click.ffmpeg").lines())
        .arg(path)
        .output()
        .expect("run FFmpeg fixture generator");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_synced_movie(path: &Path, start_frame: u64, uncut_frames: usize) {
    let samples = decode_output(path, &["-vn", "-ac", "1", "-ar", "48000", "-f", "f32le"]);
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

    let video = decode_output(path, &["-an", "-pix_fmt", "rgb24", "-f", "rawvideo"]);
    let (frames, remainder) = video.as_chunks::<{ 128 * 72 * 3 }>();
    assert!(remainder.is_empty());
    assert_eq!(frames.len(), uncut_frames - start_frame as usize);
    let mut frames = frames.iter();
    let first = frames.next().unwrap();
    let center = (36 * 128 + 64) * 3;
    assert_eq!(
        first[center..][..3].iter().any(|&pixel| pixel > 25),
        start_frame != 0
    );
    let flash = frames
        .position(|frame| {
            frame[(36 * 128 + 64) * 3..][..3]
                .iter()
                .all(|&pixel| pixel > 170)
        })
        .expect("source flash must appear in the rendered clip")
        + 1;
    let visual_time = flash as f64 * (1001.0 / 30_000.0);
    assert!(
        (visual_time - actual).abs() < 1001.0 / 30_000.0,
        "flash at {visual_time}s and audio click at {actual}s"
    );
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
