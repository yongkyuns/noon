use super::*;
use std::fs;
use noon::integration::{ExportFrame, FrameGrid, SampleObservation};
use crate::CaptureWork;

fn temporary() -> Destination {
    Destination::new(&std::env::temp_dir().join("noon-video-unit-test-anchor.mp4"), false).unwrap()
}

fn rgba() -> Vec<u8> { [27, 131, 219, 255].repeat(64 * 48) }

fn sample(rgba: &[u8]) -> CapturedFrame<'_> {
    let rate = FrameRate::new(30, 1).unwrap();
    let source = FrameGrid::new(rate, 0.0).unwrap().sample(4).unwrap();
    let mut scene = noon::Scene::new();
    let session = scene.execution_session().unwrap();
    CapturedFrame {
        frame: ExportFrame { source_sample: source, pts: 0, held: false },
        observation: SampleObservation { requested_time: source.authored_time(), published_time: source.authored_time(), publication: session.publication_context() },
        width: 64, height: 48, format: CapturePixelFormat::RendererRgba8UnormOpaque,
        rgba, work: CaptureWork::default(),
    }
}

fn unstarted(destination: Destination) -> Mp4Encoder {
    Mp4Encoder {
        options: VideoOptions::mp4(&destination.path), destination, process: None,
        cancellation: CaptureCancellation::default(), width: 64, height: 48,
        rate: FrameRate::new(30, 1).unwrap(), frame_bytes: 64 * 48 * 4,
        frames: 0, last_source_index: None, failed: false,
    }
}

#[test]
fn unsupported_configuration_is_rejected_before_starting_an_encoder() {
    let mut options = VideoOptions::mp4("unused.mp4");
    let rate = FrameRate::new(30, 1).unwrap();
    for (width, height) in [(0, 64), (64, 0), (65, 48), (64, 49), (u32::MAX - 1, 48)] {
        assert!(options.validate(width, height, rate).is_err());
    }
    assert!(options.validate(64, 48, FrameRate::new(u32::MAX, 1).unwrap()).is_err());
    options.preset = "invalid".into();
    assert!(options.validate(64, 48, rate).is_err());
    options.preset = "medium".into();
    options.crf = 52;
    assert!(options.validate(64, 48, rate).is_err());
    options.crf = 18;
    options.write_timeout = Duration::ZERO;
    assert!(options.validate(64, 48, rate).is_err());
}

#[test]
fn no_clobber_is_atomic_even_when_a_destination_appears_during_encoding() {
    let root = temporary();
    let path = root.stage.join("movie.mp4");
    let target = Destination::new(&path, false).unwrap();
    fs::write(target.temporary_video(), b"complete candidate").unwrap();
    fs::write(&path, b"concurrent existing file").unwrap();
    assert!(target.publish().is_err());
    assert_eq!(fs::read(&path).unwrap(), b"concurrent existing file");
    drop(target);
    assert_eq!(fs::read(&path).unwrap(), b"concurrent existing file");
}

#[test]
fn overwrite_is_deferred_and_dropping_an_incomplete_run_preserves_the_old_file() {
    let root = temporary();
    let path = root.stage.join("movie.mp4");
    fs::write(&path, b"old video").unwrap();
    assert!(Destination::new(&path, false).is_err());
    {
        let target = Destination::new(&path, true).unwrap();
        fs::write(target.temporary_video(), b"incomplete").unwrap();
    }
    assert_eq!(fs::read(&path).unwrap(), b"old video");
    {
        let target = Destination::new(&path, true).unwrap();
        fs::write(target.temporary_video(), b"complete replacement").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"old video");
        assert_eq!(target.publish().unwrap(), 20);
    }
    assert_eq!(fs::read(&path).unwrap(), b"complete replacement");
    assert_eq!(fs::read_dir(&root.stage).unwrap().count(), 1);
}

#[test]
fn new_output_is_not_visible_until_publication_and_survives_staging_cleanup() {
    let root = temporary();
    let path = root.stage.join("name with spaces; no shell.mp4");
    {
        let target = Destination::new(&path, false).unwrap();
        fs::write(target.temporary_video(), b"final bytes").unwrap();
        assert!(!path.exists());
        target.publish().unwrap();
    }
    assert_eq!(fs::read(&path).unwrap(), b"final bytes");
    let target = Destination::new(&root.stage.join("empty.mp4"), false).unwrap();
    fs::write(target.temporary_video(), b"").unwrap();
    assert!(target.publish().is_err());
    assert!(!target.path.exists());
}

#[cfg(unix)]
#[test]
fn overwrite_rejects_symlinks_and_directories() {
    let root = temporary();
    let real = root.stage.join("real.mp4");
    fs::write(&real, b"original").unwrap();
    let link = root.stage.join("link.mp4");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert!(Destination::new(&link, true).is_err());
    assert!(Destination::new(&root.stage, true).is_err());
    assert_eq!(fs::read(real).unwrap(), b"original");
}

#[test]
fn frame_contract_preserves_cropped_source_identity_and_rejects_bad_metadata() {
    let root = temporary();
    let mut encoder = unstarted(Destination::new(&root.stage.join("movie.mp4"), false).unwrap());
    let pixels = rgba();
    let mut frame = sample(&pixels);
    assert!(encoder.validate_frame(&frame).is_ok());
    frame.frame.pts = 1;
    assert!(encoder.validate_frame(&frame).is_err());
    frame.frame.pts = 0;
    frame.width = 66;
    assert!(encoder.validate_frame(&frame).is_err());
    frame.width = 64;
    frame.observation.published_time = 0.0;
    assert!(encoder.validate_frame(&frame).is_err());
    frame.frame.held = true;
    assert!(encoder.validate_frame(&frame).is_ok());
    frame.observation.published_time = f64::NAN;
    assert!(encoder.validate_frame(&frame).is_err());
    frame = sample(&pixels);
    encoder.last_source_index = Some(2);
    assert!(encoder.validate_frame(&frame).is_err());
    encoder.last_source_index = Some(3);
    assert!(encoder.validate_frame(&frame).is_ok());
    encoder.rate = FrameRate::new(60_000, 1_001).unwrap();
    assert!(encoder.validate_frame(&frame).is_err());
}

#[test]
fn alpha_and_length_errors_are_terminal_and_never_publish_output() {
    let root = temporary();
    let path = root.stage.join("movie.mp4");
    let mut encoder = unstarted(Destination::new(&path, false).unwrap());
    let mut pixels = rgba();
    pixels[3] = 254;
    assert!(encoder.write_frame(&sample(&pixels)).is_err());
    pixels[3] = 255;
    assert!(matches!(encoder.write_frame(&sample(&pixels)), Err(VideoError::Inactive)));
    assert!(encoder.finish(0).is_err());
    assert!(!path.exists());
    let encoder = unstarted(Destination::new(&path, false).unwrap());
    assert!(encoder.validate_frame(&sample(&pixels[..4])).is_err());
}

#[test]
fn missing_encoder_and_pre_cancelled_output_leave_no_destination() {
    let root = temporary();
    let path = root.stage.join("movie.mp4");
    let mut options = VideoOptions::mp4(&path);
    options.ffmpeg = root.stage.join("missing-ffmpeg");
    assert!(Mp4Encoder::new(options.clone(), 64, 48, FrameRate::new(30, 1).unwrap(), CaptureCancellation::default()).is_err());
    let cancel = CaptureCancellation::default();
    cancel.cancel();
    assert!(matches!(Mp4Encoder::new(options, 64, 48, FrameRate::new(30, 1).unwrap(), cancel), Err(VideoError::Cancelled)));
    assert!(!path.exists());
    assert_eq!(fs::read_dir(&root.stage).unwrap().count(), 0);
}
