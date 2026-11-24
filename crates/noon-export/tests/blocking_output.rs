//! Source-once ownership, failure cleanup and independently decoded ordinary output.
#![cfg(not(target_arch = "wasm32"))]

use std::{
    cell::{Cell, RefCell},
    fs,
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
use noon::{BlockingScene, Color, ScriptError};
use noon_export::output::{
    export_blocking_scene, export_blocking_scene_with_limits, BlockingFileExportError,
    BlockingSourceLimits, OutputOptions,
};
use noon_export::{Backends, CaptureOptions};

static NEXT: AtomicU64 = AtomicU64::new(0);
const FRAME_BYTES: usize = 320 * 180 * 3;

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "noon-blocking-output-{}-{}",
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

fn frames() -> ExportFrameOptions {
    ExportFrameOptions {
        frame_rate: FrameRate::new(30, 1).unwrap(),
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
        ..CaptureOptions::new(320, 180)
    }
}

fn assert_retired(source: &RefCell<Option<BlockingScene>>) {
    let source = source.borrow();
    let source = source.as_ref().unwrap();
    assert!(source.time().is_err());
    assert!(source.square(0.1).is_err());
    assert!(source.wait(0.0).is_err());
}

#[test]
fn invalid_options_and_pre_cancelled_calls_never_execute_source() {
    let root = Directory::new();
    for cancelled in [false, true] {
        let calls = Cell::new(0);
        let mut options = capture();
        if cancelled {
            options.cancellation.cancel();
        } else {
            options.width = 0;
        }
        assert!(export_blocking_scene(
            |_| {
                calls.set(calls.get() + 1);
                Ok(())
            },
            frames(),
            options,
            OutputOptions::mp4(root.0.join("movie.mp4"))
        )
        .is_err());
        assert_eq!(calls.get(), 0);
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
    }
}

#[test]
fn source_failure_and_unwind_retire_clones_without_touching_existing_output() {
    let root = Directory::new();
    let path = root.0.join("movie.mp4");
    fs::write(&path, b"existing movie").unwrap();
    for panics in [false, true] {
        let stale = RefCell::new(None);
        let mut output = OutputOptions::mp4(&path);
        output.overwrite = true;
        let result = catch_unwind(AssertUnwindSafe(|| {
            export_blocking_scene(
                |scene| {
                    stale.replace(Some(scene.clone()));
                    let square = scene.square(0.8)?;
                    scene.add(&square)?;
                    scene.wait(0.25)?;
                    if panics {
                        panic!("application source panic");
                    }
                    Err(ScriptError::Host("application failure".into()))
                },
                frames(),
                capture(),
                output,
            )
        }));
        if panics {
            assert!(result.is_err());
        } else {
            assert!(matches!(
                result.unwrap(),
                Err(BlockingFileExportError::Source(_))
            ));
        }
        assert_retired(&stale);
        assert_eq!(fs::read(&path).unwrap(), b"existing movie");
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
    }
}

#[test]
fn caught_preparation_limit_still_prevents_export() {
    let root = Directory::new();
    let stale = RefCell::new(None);
    let result = export_blocking_scene_with_limits(
        |scene| {
            stale.replace(Some(scene.clone()));
            scene.wait(0.0)?;
            assert!(scene.wait(0.0).is_err());
            Ok(())
        },
        frames(),
        capture(),
        OutputOptions::png_sequence(root.0.join("images")),
        BlockingSourceLimits {
            max_boundaries: 1,
            ..Default::default()
        },
    );
    assert!(matches!(
        result,
        Err(BlockingFileExportError::SourceLimit {
            kind: "boundaries",
            limit: 1
        })
    ));
    assert_retired(&stale);
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
}

fn decode(path: &Path, png: bool) -> Vec<u8> {
    let mut command = Command::new("ffmpeg");
    command.args(["-v", "error"]);
    if png {
        command
            .args(["-framerate", "30", "-i"])
            .arg(path.join("frames/frame-%010d.png"));
    } else {
        command.arg("-i").arg(path);
    }
    let output = command
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[cfg(all(feature = "native-text", feature = "typst", feature = "bundled-fonts"))]
#[test]
#[ignore = "requires software Vulkan and FFmpeg"]
fn unchanged_first_lesson_runs_once_and_decodes_complete_png_and_mp4() {
    let root = Directory::new();
    for png in [true, false] {
        let path = root.0.join(if png { "images" } else { "lesson.mp4" });
        let calls = Cell::new(0);
        let stale = RefCell::new(None);
        let output = if png {
            OutputOptions::png_sequence(&path)
        } else {
            OutputOptions::mp4(&path)
        };
        let summary = export_blocking_scene(
            |scene| {
                calls.set(calls.get() + 1);
                stale.replace(Some(scene.clone()));
                noon::example_scenes::first_lesson::construct(scene)
            },
            frames(),
            capture(),
            output,
        )
        .unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(summary.capture.sampling.frames, 60);
        assert_retired(&stale);
        if png {
            assert!(!path.join(".incomplete").exists());
        }
        let decoded = decode(&path, png);
        assert_eq!(decoded.len(), 60 * 320 * 180 * 3);
        // Independent image-space observations of the documented source. Check
        // both text regions, both shapes' simultaneous motion, and late presence
        // on every decoded frame, rather than trusting the exporter summary.
        for (index, frame) in decoded.as_chunks::<FRAME_BYTES>().0.iter().enumerate() {
            let (mut title, mut equation, mut caption) = (0, 0, 0);
            let (mut square_count, mut circle_count) = (0, 0);
            let (mut square_x, mut circle_x) = (0_u64, 0_u64);
            for (pixel_index, pixel) in frame.as_chunks::<3>().0.iter().enumerate() {
                let [r, g, b] = *pixel;
                let (x, y) = (pixel_index % 320, pixel_index / 320);
                if r < 140 && g > 120 && b > 170 {
                    square_count += 1;
                    square_x += x as u64;
                }
                if r > 190 && g > 70 && g < 180 && b < 100 {
                    circle_count += 1;
                    circle_x += x as u64;
                }
                if r.min(g).min(b) > 160 {
                    title += usize::from((y as f64) < 180.0 * 0.235);
                    equation +=
                        usize::from((y as f64) >= 180.0 * 0.235 && (y as f64) < 180.0 * 0.4);
                    caption += usize::from((y as f64) >= 180.0 * 0.62);
                }
            }
            assert!(
                title >= 20 && equation >= 15,
                "text frame {index}, PNG={png}"
            );
            if index < 45 {
                assert_eq!(caption, 0, "early caption frame {index}");
            } else {
                assert!(caption >= 15, "missing caption frame {index}");
            }
            let shift = 1.5 * (index as f64 / 30.0 - 0.5).clamp(0.0, 1.0);
            for (count, x, initial) in [
                (square_count, square_x, -0.8),
                (circle_count, circle_x, 0.8),
            ] {
                assert!(count >= 50, "missing shape frame {index}");
                let expected = 319.0 / 2.0 + 22.5 * (initial + shift);
                assert!(
                    (x as f64 / count as f64 - expected).abs() <= 1.25,
                    "shape motion frame {index}, PNG={png}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires software Vulkan and FFmpeg"]
fn no_wait_static_source_exports_an_explicit_hold_and_output_failure_retires_source() {
    let root = Directory::new();
    let stale = RefCell::new(None);
    let path = root.0.join("static");
    let mut sampling = frames();
    sampling.final_hold_seconds = 0.1;
    let summary = export_blocking_scene(
        |scene| {
            stale.replace(Some(scene.clone()));
            let square = scene.square(0.8)?;
            scene.set_fill(&square, Color::WHITE, 1.0)?;
            scene.add(&square)
        },
        sampling,
        capture(),
        OutputOptions::png_sequence(&path),
    )
    .unwrap();
    assert_eq!(summary.capture.sampling.frames, 3);
    assert_retired(&stale);
    let decoded = decode(&path, true);
    assert_eq!(decoded.len(), 3 * 320 * 180 * 3);
    assert!(decoded
        .as_chunks::<3>()
        .0
        .iter()
        .any(|pixel| pixel[0] > 200));
    for frame in decoded.as_chunks::<FRAME_BYTES>().0.iter().skip(1) {
        assert_eq!(frame, &decoded[..320 * 180 * 3]);
    }

    let failure_stale = RefCell::new(None);
    let existing = root.0.join("existing.mp4");
    fs::write(&existing, b"keep existing").unwrap();
    let mut output = OutputOptions::mp4(&existing);
    output.overwrite = true;
    output.ffmpeg = root.0.join("absent-ffmpeg");
    assert!(matches!(
        export_blocking_scene(
            |scene| {
                failure_stale.replace(Some(scene.clone()));
                scene.wait(0.1)
            },
            frames(),
            capture(),
            output
        ),
        Err(BlockingFileExportError::Export(_))
    ));
    assert_retired(&failure_stale);
    assert_eq!(fs::read(&existing).unwrap(), b"keep existing");
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 2);
}
