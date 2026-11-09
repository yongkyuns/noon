//! Admission and cleanup for the shared native prepared-scene export boundary.
#![cfg(not(target_arch = "wasm32"))]

use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
use noon::{AnimationOptions, AudioClipOptions, AudioResource, Mobject, Scene, SceneExecution};
use noon_core::{
    NativeInputModifiers, NativePointerId, NativePointerInput, NativePointerInputKind,
    NativePointerPosition, Vec2,
};
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

fn static_click_scene(clicked: bool) -> (SceneExecution, Mobject) {
    let mut scene = Scene::new();
    let mut marker = scene.circle(0.5).unwrap();
    marker.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    marker.disable_stroke().unwrap();
    scene.add(&marker).unwrap();
    let pulse = scene
        .declare_glow_pulse(&marker, 2.0, AnimationOptions::new().run_time(1.0))
        .unwrap();
    scene.on_click(&marker, Some(&pulse)).unwrap();
    let mut execution = scene.into_execution().unwrap();
    if clicked {
        execution
            .session_mut()
            .configure_native_pointer_input(
                NativePointerId {
                    source: 61,
                    pointer: 1,
                },
                1,
            )
            .unwrap();
        let position = NativePointerPosition::new(Vec2::ZERO, Vec2::new(80.0, 45.0)).unwrap();
        for (sequence, kind) in [
            (
                1,
                NativePointerInputKind::Press {
                    position,
                    button: 0,
                },
            ),
            (
                2,
                NativePointerInputKind::Release {
                    position,
                    button: 0,
                },
            ),
        ] {
            let token = execution.session().native_pointer_input_token().unwrap();
            let input = NativePointerInput::new(
                sequence,
                token.pointer(),
                token.context(),
                NativeInputModifiers::default(),
                kind,
            );
            execution
                .submit_native_pointer_input(&token, input)
                .unwrap();
        }
        execution.advance_interactions(0.0).unwrap();
        execution.advance_interactions(0.5).unwrap();
        assert!(execution.session().interactions_active());
        assert!(marker.get_effect("glow").is_ok());
    }
    assert_eq!(execution.session().frame().time, 0.0);
    assert_eq!(execution.session().finite_playback_end_time(), Some(0.0));
    (execution, marker)
}

fn decode_png_sequence(path: &Path) -> Vec<u8> {
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-framerate", "8", "-i"])
        .arg(path.join("frames/frame-%010d.png"))
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
#[ignore = "requires software Vulkan and FFmpeg"]
fn authored_zero_export_retires_click_effect_before_every_frozen_hold_frame() {
    let root = Directory::new();
    let options = ExportFrameOptions {
        frame_rate: FrameRate::new(8, 1).unwrap(),
        final_hold_seconds: 0.5,
        ..frames()
    };
    let mut movies = Vec::new();
    for clicked in [false, true] {
        let (mut execution, marker) = static_click_scene(clicked);
        let path = root.0.join(if clicked { "clicked" } else { "authored" });
        let summary = export_scene_file(
            &mut execution,
            options,
            capture(),
            OutputOptions::png_sequence(&path),
        )
        .unwrap();
        assert_eq!(summary.capture.sampling.frames, 4);
        assert_eq!(summary.capture.sampling.source_end, Some(0.0));
        movies.push(decode_png_sequence(&path));
        assert!(!execution.session().interactions_active());
        assert!(marker.get_effect("glow").is_err());
    }
    let frame_bytes = 160 * 90 * 4;
    assert_eq!(movies[0].len(), 4 * frame_bytes);
    assert_eq!(movies[1].len(), movies[0].len());
    assert!(movies[0].chunks_exact(4).any(|pixel| pixel[0] > 200));
    for (index, (authored, clicked)) in movies[0]
        .chunks_exact(frame_bytes)
        .zip(movies[1].chunks_exact(frame_bytes))
        .enumerate()
    {
        assert_eq!(
            clicked, authored,
            "input-triggered glow leaked into held output frame {index}"
        );
    }
}
