use super::*;
use crate::CaptureWork;
use noon::integration::{ExportFrame, FrameGrid, SampleObservation};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "noon-output-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn rate() -> FrameRate {
    FrameRate::new(60_000, 1_001).unwrap()
}

#[test]
fn rejects_unsupported_dimensions_quality_and_overwrite_policy() {
    let mut options = OutputOptions::mp4("unused.mp4");
    for (width, height) in [(0, 2), (2, 0), (3, 2), (2, 3)] {
        assert!(options.validate(rate(), width, height).is_err());
    }
    options.crf = 52;
    assert!(options.validate(rate(), 2, 2).is_err());
    options.crf = 18;
    options.io_timeout = Duration::ZERO;
    assert!(options.validate(rate(), 2, 2).is_err());
    let mut png = OutputOptions::png_sequence("unused");
    assert!(png.validate(rate(), 65, 33).is_ok());
    png.overwrite = true;
    assert!(png.validate(rate(), 65, 33).is_err());
}

#[test]
fn mp4_publication_is_no_clobber_even_when_destination_appears_late() {
    let root = Temp::new();
    let path = root.0.join("race.mp4");
    let destination = Destination::new(&OutputOptions::mp4(&path)).unwrap();
    let work = destination.work.clone();
    fs::write(work.join("video.mp4"), b"candidate").unwrap();
    fs::write(&path, b"concurrent owner").unwrap();
    assert_eq!(
        destination.publish(1).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(&path).unwrap(), b"concurrent owner");
    assert!(!work.exists());
}

#[test]
fn abort_preserves_old_video_and_only_successful_explicit_overwrite_replaces_it() {
    let root = Temp::new();
    let path = root.0.join("keep.mp4");
    fs::write(&path, b"old").unwrap();
    assert!(Destination::new(&OutputOptions::mp4(&path)).is_err());
    let mut options = OutputOptions::mp4(&path);
    options.overwrite = true;
    {
        let destination = Destination::new(&options).unwrap();
        fs::write(destination.work.join("video.mp4"), b"partial").unwrap();
    }
    assert_eq!(fs::read(&path).unwrap(), b"old");
    let destination = Destination::new(&options).unwrap();
    fs::write(destination.work.join("video.mp4"), b"complete").unwrap();
    destination.publish(1).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"complete");
}

#[test]
fn png_reservation_does_not_replace_existing_directories_and_abort_removes_only_ours() {
    let root = Temp::new();
    let path = root.0.join("images");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("owned.txt"), b"keep").unwrap();
    assert!(Destination::new(&OutputOptions::png_sequence(&path)).is_err());
    assert_eq!(fs::read(path.join("owned.txt")).unwrap(), b"keep");
    let new = root.0.join("new-images");
    {
        let destination = Destination::new(&OutputOptions::png_sequence(&new)).unwrap();
        assert!(destination.work.exists());
        assert!(!new.join("frames").exists());
    }
    assert!(!new.exists());
}

#[test]
fn png_completed_frames_and_manifest_are_published_together() {
    let root = Temp::new();
    let path = root.0.join("images");
    let destination = Destination::new(&OutputOptions::png_sequence(&path)).unwrap();
    let frames = destination.work.join("frames");
    fs::write(frames.join("frame-0000000000.png"), b"fixture").unwrap();
    fs::write(frames.join("timing.tsv"), b"complete").unwrap();
    destination.publish(1).unwrap();
    assert!(!path.join(".incomplete").exists());
    assert_eq!(
        fs::read(path.join("frames/timing.tsv")).unwrap(),
        b"complete"
    );
}

#[test]
fn nonexistent_ffmpeg_fails_and_cleans_staging_without_a_gpu() {
    let root = Temp::new();
    let path = root.0.join("video.mp4");
    let mut options = OutputOptions::mp4(&path);
    options.ffmpeg = root.0.join("missing-encoder");
    assert!(FileSink::new(options, rate(), 4, 4, CaptureCancellation::default()).is_err());
    assert!(!path.exists());
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
}

fn frame<'a>(pixels: &'a [u8], width: u32, height: u32, index: u64) -> CapturedFrame<'a> {
    let mut scene = noon::Scene::new();
    let object = scene.square(1.0).unwrap();
    scene.add(&object).unwrap();
    let session = scene.execution_session().unwrap();
    let sample = FrameGrid::new(rate(), 0.0).unwrap().sample(index).unwrap();
    CapturedFrame {
        frame: ExportFrame {
            source_sample: sample,
            pts: index,
            held: false,
        },
        observation: SampleObservation {
            requested_time: sample.authored_time(),
            published_time: sample.authored_time(),
            publication: session.publication_context(),
        },
        width,
        height,
        format: CapturePixelFormat::RendererRgba8UnormOpaque,
        rgba: pixels,
        work: CaptureWork::default(),
    }
}

#[test]
#[ignore = "requires FFmpeg; selected by native output gate"]
fn png_sink_roundtrips_odd_size_bytes_and_records_fractional_timing() {
    let root = Temp::new();
    let mut sink = FileSink::new(
        OutputOptions::png_sequence(root.0.join("images")),
        rate(),
        65,
        33,
        CaptureCancellation::default(),
    )
    .unwrap();
    let mut expected = Vec::new();
    for index in 0..4 {
        let mut pixels = Vec::new();
        for y in 0..33 {
            for x in 0..65 {
                pixels.extend_from_slice(&[
                    (x + index * 17) as u8,
                    (y * 7) as u8,
                    (x + y) as u8,
                    255,
                ]);
            }
        }
        sink.write(frame(&pixels, 65, 33, index)).unwrap();
        expected.push(pixels);
    }
    let (path, _, _) = sink.finish(4).unwrap();
    for (index, expected) in expected.iter().enumerate() {
        let output = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(path.join(format!("frames/frame-{index:010}.png")))
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(&output.stdout, expected);
    }
    let manifest = fs::read_to_string(path.join("frames/timing.tsv")).unwrap();
    assert!(manifest.contains("fps=60000/1001"));
    assert!(manifest.ends_with("# complete frames=4\n"));
}

#[test]
#[ignore = "requires FFmpeg; selected by native output gate"]
fn rejected_frames_poison_output_and_never_publish() {
    let root = Temp::new();
    for bad in 0..12 {
        let path = root.0.join(format!("bad-{bad}.mp4"));
        let mut sink = FileSink::new(
            OutputOptions::mp4(&path),
            rate(),
            4,
            4,
            CaptureCancellation::default(),
        )
        .unwrap();
        let mut pixels = vec![255; 4 * 4 * 4];
        if bad == 1 {
            pixels[3] = 128;
        }
        if bad == 2 {
            pixels.pop();
        }
        let index = if bad == 0 { 1 } else { 0 };
        let mut invalid = frame(&pixels, 4, 4, index);
        match bad {
            3 => invalid.observation.requested_time = 0.01,
            4 => invalid.observation.published_time = f64::NAN,
            5 => invalid.observation.published_time = f64::INFINITY,
            6 => invalid.observation.published_time = -1.0,
            7 => invalid.observation.published_time = 1.0,
            8 => {
                // A crop may rebase PTS, but a dynamic sample cannot reuse an
                // older published state. Only an explicit terminal hold can.
                let next = FrameGrid::new(rate(), 0.0).unwrap().sample(1).unwrap();
                invalid.frame.source_sample = next;
                invalid.observation.requested_time = next.authored_time();
            }
            9 => {
                let wrong_rate = FrameRate::new(30, 1).unwrap();
                invalid.frame.source_sample =
                    FrameGrid::new(wrong_rate, 0.0).unwrap().sample(0).unwrap();
            }
            10 => invalid.width = 5,
            11 => invalid.height = 5,
            _ => {}
        }
        assert!(sink.write(invalid).is_err(), "invalid input case {bad}");
        // Rejection is terminal, even if the next supplied sample is valid.
        assert!(sink.write(frame(&[255; 64], 4, 4, 0)).is_err());
        assert!(sink.finish(1).is_err());
        assert!(!path.exists());
    }
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
}

#[test]
#[ignore = "requires Python3; selected by native output gate"]
fn encoder_drains_and_bounds_diagnostics_and_reports_nonzero_exit() {
    let mut command = Command::new("python3");
    command.args(["-c", "import sys; sys.stderr.write('x'*200000+'TAIL'); sys.stderr.flush(); sys.stdin.buffer.read(); sys.exit(7)"]);
    let mut encoder = Encoder::new(
        &mut command,
        16,
        CaptureCancellation::default(),
        Duration::from_secs(10),
    )
    .unwrap();
    encoder.write(&[0; 16]).unwrap();
    let message = encoder.finish().unwrap_err().to_string();
    assert!(message.contains("TAIL"));
    assert!(message.len() < 66_000);
    assert!(message.contains('7'));
}

#[test]
#[ignore = "requires Python3; selected by native output gate"]
fn encoder_write_timeout_kills_and_reaps_blocked_process() {
    let mut command = Command::new("python3");
    command.args(["-c", "import time; time.sleep(60)"]);
    let mut encoder = Encoder::new(
        &mut command,
        1024 * 1024,
        CaptureCancellation::default(),
        Duration::from_millis(100),
    )
    .unwrap();
    let start = std::time::Instant::now();
    assert_eq!(
        encoder.write(&vec![0; 1024 * 1024]).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    drop(encoder);
    assert!(start.elapsed() < Duration::from_secs(10));
}

#[test]
#[ignore = "requires Python3; selected by native output gate"]
fn encoder_flush_timeout_and_cancellation_do_not_report_success() {
    let mut command = Command::new("python3");
    command.args([
        "-c",
        "import sys,time; sys.stdin.buffer.read(); time.sleep(60)",
    ]);
    let mut encoder = Encoder::new(
        &mut command,
        16,
        CaptureCancellation::default(),
        Duration::from_millis(200),
    )
    .unwrap();
    encoder.write(&[0; 16]).unwrap();
    assert_eq!(
        encoder.finish().unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    let cancellation = CaptureCancellation::default();
    let mut command = Command::new("python3");
    command.args(["-c", "import time; time.sleep(60)"]);
    let mut encoder = Encoder::new(
        &mut command,
        16,
        cancellation.clone(),
        Duration::from_secs(10),
    )
    .unwrap();
    cancellation.cancel();
    assert_eq!(
        encoder.write(&[0; 16]).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
}
