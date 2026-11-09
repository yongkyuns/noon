//! Admission and cleanup for the shared native prepared-scene export boundary.
#![cfg(not(target_arch = "wasm32"))]

use std::{
    fs, io,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
use noon::{AudioClipOptions, AudioResource, Scene, SceneExecution};
use noon_export::output::{
    export_scene_file, export_scene_file_with_interrupt, OutputOptions, SessionFileExportError,
};
use noon_export::{Backends, CaptureOptions};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "noon-scene-output-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture(sealed: bool, audio: bool) -> SceneExecution {
    let mut scene = Scene::new();
    let mut marker = scene.square(0.8).unwrap();
    marker.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    marker.disable_stroke().unwrap();
    scene.add(&marker).unwrap();
    if audio {
        scene
            .audio(
                AudioResource::from_interleaved(48_000, 1, vec![0.0; 480], 4096).unwrap(),
                AudioClipOptions::default(),
            )
            .unwrap();
    }
    let mut session = scene.execution_session().unwrap();
    session.begin_replay_retention(Default::default()).unwrap();
    {
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.1).unwrap();
        live.advance_segment_to(wait, wait.end_time()).unwrap();
        live.complete_segment(wait).unwrap();
        live.set_translation(&marker, 1.0, 0.0).unwrap();
        let wait = live.wait_segment(0.13).unwrap();
        live.advance_segment_to(wait, wait.end_time()).unwrap();
        live.complete_segment(wait).unwrap();
    }
    if sealed {
        session.seal_replay_for_viewer().unwrap();
    }
    scene.into_execution_with_session(session).unwrap()
}

fn frames() -> ExportFrameOptions {
    ExportFrameOptions {
        frame_rate: FrameRate::new(30_000, 1_001).unwrap(),
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 128,
        max_transitions_per_sample: 64,
        final_hold_seconds: 0.0,
    }
}

fn capture() -> CaptureOptions {
    CaptureOptions {
        backends: Backends::VULKAN,
        force_fallback_adapter: true,
        ..CaptureOptions::new(160, 90)
    }
}

#[test]
fn unsealed_history_and_history_audio_cannot_create_output() {
    let root = Directory::new();
    for (sealed, audio) in [(false, false), (true, true)] {
        let mut scene = fixture(sealed, audio);
        let publication = scene.session().publication_context();
        let error = export_scene_file(
            &mut scene,
            frames(),
            capture(),
            OutputOptions::mp4(root.0.join("movie.mp4")),
        )
        .unwrap_err();
        assert!(
            matches!(
                error,
                SessionFileExportError::UnsupportedHistory | SessionFileExportError::HistoryAudio
            ),
            "{error}"
        );
        assert_eq!(scene.session().publication_context(), publication);
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
    }
}

#[test]
fn invalid_range_and_cancelled_export_do_not_rewind_or_touch_destination() {
    let root = Directory::new();
    let path = root.0.join("movie.mp4");
    fs::write(&path, b"previous user movie").unwrap();
    for cancelled in [false, true] {
        let mut scene = fixture(true, false);
        let publication = scene.session().publication_context();
        let options = if cancelled {
            frames()
        } else {
            ExportFrameOptions {
                max_frames: 1,
                ..frames()
            }
        };
        let capture = capture();
        if cancelled {
            capture.cancellation.cancel();
        }
        let mut output = OutputOptions::mp4(&path);
        output.overwrite = true;
        output.ffmpeg = root.0.join("must-not-run");
        assert!(export_scene_file(&mut scene, options, capture, output).is_err());
        assert_eq!(scene.session().publication_context(), publication);
        assert_eq!(fs::read(&path).unwrap(), b"previous user movie");
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
    }
}

#[test]
#[ignore = "requires software Vulkan and FFmpeg"]
fn host_interrupt_after_capture_abandons_encoder_and_preserves_destination() {
    let root = Directory::new();
    let path = root.0.join("movie.mp4");
    fs::write(&path, b"previous user movie").unwrap();
    let mut scene = fixture(true, false);
    let mut output = OutputOptions::mp4(&path);
    output.overwrite = true;
    let mut checks = 0;
    let error = export_scene_file_with_interrupt(&mut scene, frames(), capture(), output, || {
        checks += 1;
        if checks >= 6 {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "host interrupted",
            ))
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert!(
        matches!(error, SessionFileExportError::Output(error) if error.kind() == io::ErrorKind::Interrupted)
    );
    assert_eq!(fs::read(&path).unwrap(), b"previous user movie");
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
}
