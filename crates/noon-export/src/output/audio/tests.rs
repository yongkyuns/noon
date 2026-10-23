use super::*;
use noon::integration::{ExportEndReason, FrameRate};
use noon::{AudioClipOptions, AudioResource, Scene};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "noon-pcm-stage-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn source() -> ExecutionSession {
    let mut scene = Scene::new();
    let pcm = AudioResource::from_interleaved(8_000, 1, vec![0.25; 8_000], 64_000).unwrap();
    scene
        .audio(
            pcm,
            AudioClipOptions {
                duration: Some(4.0),
                looped: true,
                ..AudioClipOptions::default()
            },
        )
        .unwrap();
    scene.execution_session().unwrap()
}

fn summary(frames: u64, rate: FrameRate, start: f64, end: f64) -> ExportFrameSummary {
    ExportFrameSummary {
        frames,
        frame_rate: rate,
        start_time: start,
        end_time: end,
        source_end: None,
        scheduled_duration: frames as f64 * f64::from(rate.denominator())
            / f64::from(rate.numerator()),
        reason: ExportEndReason::RequestedStop,
    }
}

#[test]
fn fractional_video_extent_uses_exact_integer_sample_ceiling() {
    let session = source();
    for (frames, p, q, expected) in [
        (1, 30_000, 1_001, 1_602),
        (70, 30_000, 1_001, 112_112),
        (120, 60_000, 1_001, 96_096),
        (1, 100, 1, 480),
    ] {
        let rate = FrameRate::new(p, q).unwrap();
        let sampling = summary(frames, rate, 0.0, 4.0);
        assert_eq!(
            Soundtrack::new(&session, sampling).unwrap().frames,
            expected
        );
    }
}

#[test]
fn unrepresentable_movie_clocks_and_durations_fail_before_staging() {
    let session = source();
    for sampling in [
        summary(90_000, FrameRate::new(1, 1).unwrap(), 0.0, 90_000.0),
        summary(1, FrameRate::new(2_000_000_003, 1).unwrap(), 0.0, 1.0),
    ] {
        assert!(Soundtrack::new(&session, sampling).is_err());
    }
    assert_eq!(session.frame().time, 0.0);
}

#[test]
fn explicit_crop_stop_pads_last_video_frame_with_silence() {
    let root = Temp::new();
    let session = source();
    let sampling = summary(1, FrameRate::new(30_000, 1_001).unwrap(), 0.25, 0.26);
    let soundtrack = Soundtrack::new(&session, sampling).unwrap();
    soundtrack
        .stage(&root.0, &CaptureCancellation::default())
        .unwrap();
    let bytes = std::fs::read(root.0.join(PCM_FILENAME)).unwrap();
    let samples = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect::<Vec<_>>();
    assert_eq!(samples.len(), 1_602 * CHANNELS);
    assert!(samples[..480 * CHANNELS]
        .iter()
        .all(|&sample| sample == 0.25));
    assert!(samples[480 * CHANNELS..]
        .iter()
        .all(|&sample| sample == 0.0));
    assert_eq!(session.frame().time, 0.0);
}

#[test]
fn staged_blocks_preserve_absolute_scene_phase_and_runtime_publication() {
    let root = Temp::new();
    let mut scene = Scene::new();
    // A quarter-second ramp, repeated. At a cropped origin of 0.125 seconds,
    // the independent first-source coordinate is exactly 1000 of 2000 frames.
    let pcm = AudioResource::from_interleaved(
        8_000,
        1,
        (0..2_000)
            .map(|index| index as f32 / 4_000.0)
            .collect::<Vec<_>>(),
        16_000,
    )
    .unwrap();
    scene
        .audio(
            pcm,
            AudioClipOptions {
                duration: Some(4.0),
                looped: true,
                ..AudioClipOptions::default()
            },
        )
        .unwrap();
    let session = scene.execution_session().unwrap();
    let publication = session.publication_context();
    let soundtrack = Soundtrack::new(
        &session,
        summary(60, FrameRate::new(30, 1).unwrap(), 0.125, 2.125),
    )
    .unwrap();
    soundtrack
        .stage(&root.0, &CaptureCancellation::default())
        .unwrap();
    let bytes = std::fs::read(root.0.join(PCM_FILENAME)).unwrap();
    assert_eq!(bytes.len(), 96_000 * CHANNELS * 4);
    for index in [0_usize, 6, 16_380, 16_386, 32_766, 48_000, 95_994] {
        let source_frame = (1_000 + index / 6) % 2_000;
        let expected = source_frame as f32 / 4_000.0;
        for channel in 0..CHANNELS {
            let offset = (index * CHANNELS + channel) * 4;
            let actual = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            assert!(
                (actual - expected).abs() < 1e-6,
                "frame {index}: {actual} != {expected}"
            );
        }
    }
    assert_eq!(session.frame().time, 0.0);
    assert_eq!(session.publication_context(), publication);
}

#[test]
fn cancelled_staging_creates_no_pcm_file() {
    let root = Temp::new();
    let session = source();
    let soundtrack = Soundtrack::new(
        &session,
        summary(30, FrameRate::new(30, 1).unwrap(), 0.0, 1.0),
    )
    .unwrap();
    let cancellation = CaptureCancellation::default();
    cancellation.cancel();
    assert_eq!(
        soundtrack.stage(&root.0, &cancellation).unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert!(!root.0.join(PCM_FILENAME).exists());
}

#[test]
fn cancellation_between_blocks_stops_before_the_next_pcm_write() {
    struct CancelAfterWrite {
        cancellation: CaptureCancellation,
        bytes: usize,
        writes: usize,
    }
    impl Write for CancelAfterWrite {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes += bytes.len();
            self.writes += 1;
            self.cancellation.cancel();
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let session = source();
    let publication = session.publication_context();
    let soundtrack = Soundtrack::new(
        &session,
        summary(60, FrameRate::new(30, 1).unwrap(), 0.0, 2.0),
    )
    .unwrap();
    let cancellation = CaptureCancellation::default();
    let mut output = CancelAfterWrite {
        cancellation: cancellation.clone(),
        bytes: 0,
        writes: 0,
    };
    assert_eq!(
        soundtrack
            .write_pcm(&mut output, &cancellation)
            .unwrap_err()
            .kind(),
        io::ErrorKind::Interrupted,
    );
    assert_eq!(output.writes, 1);
    assert_eq!(output.bytes, MAX_AUDIO_MIX_BLOCK_FRAMES * CHANNELS * 4);
    assert_eq!(session.frame().time, 0.0);
    assert_eq!(session.publication_context(), publication);
}
