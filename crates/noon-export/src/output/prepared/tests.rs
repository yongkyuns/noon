use super::*;
use noon::integration::ExportSampleKind;
use std::sync::atomic::{AtomicUsize, Ordering};

mod media;

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "noon-prepared-export-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn options() -> ExportFrameOptions {
    ExportFrameOptions {
        frame_rate: FrameRate::new(30_000, 1_001).unwrap(),
        start_frame: 3,
        stop: ExportStop::SourceEnd,
        max_frames: 100,
        max_transitions_per_sample: 64,
        final_hold_seconds: 0.0,
    }
}

fn manifest(options: ExportFrameOptions, end: f64, audio: bool) -> PreparedCaptureManifest {
    let mut policy = ExportFramePolicy::new(options).unwrap();
    let mut samples = Vec::new();
    let mut epoch = 1;
    let mut previous_time = 0.0;
    loop {
        match policy.status().unwrap() {
            ExportFramePolicyStatus::NeedsSample(requested_time) => {
                let published_time = requested_time.min(end);
                if published_time != previous_time {
                    epoch += 1;
                }
                let sample = SampleObservation {
                    requested_time,
                    published_time,
                    publication: PublicationContext::new(
                        SceneRevision::new(u64::MAX - 1),
                        ExecutionRevision::new(u64::MAX),
                        FrameEpoch::new(epoch),
                    ),
                };
                samples.push(sample.into());
                previous_time = published_time;
                policy.observe(sample, requested_time >= end).unwrap();
            }
            ExportFramePolicyStatus::SampleReady(sample) => {
                policy.acknowledge_sample(sample).unwrap()
            }
            ExportFramePolicyStatus::Complete(_) => break,
        }
    }
    PreparedCaptureManifest {
        format: PreparedCaptureManifest::FORMAT.into(),
        width: 4,
        height: 4,
        source_end: end,
        frame_options: options.into(),
        audio,
        samples,
    }
}

fn write_manifest(root: &Path, manifest: &PreparedCaptureManifest) {
    fs::write(
        root.join(MANIFEST_FILENAME),
        serde_json::to_vec(manifest).unwrap(),
    )
    .unwrap();
}

fn write_media(root: &Path, manifest: &PreparedCaptureManifest) -> ExportFrameSummary {
    write_manifest(root, manifest);
    let sampling = walk_samples(manifest, &CaptureCancellation::default(), |_| Ok(())).unwrap();
    fs::write(
        root.join(FRAMES_FILENAME),
        [30, 80, 140, 255]
            .repeat((manifest.width * manifest.height) as usize * sampling.frames as usize),
    )
    .unwrap();
    if manifest.audio {
        let frames = audio::soundtrack_grid(sampling).unwrap().frames();
        fs::write(
            root.join(PCM_FILENAME),
            vec![0_u8; frames as usize * CHANNELS * 4],
        )
        .unwrap();
    }
    sampling
}

fn missing_encoder(root: &Path) -> OutputOptions {
    let mut options = OutputOptions::mp4(root.join("output.mp4"));
    options.ffmpeg = root.join("missing-ffmpeg");
    options
}

fn assert_no_staging(root: &Path) {
    assert!(fs::read_dir(root).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".noon-export-")
    }));
}

#[test]
fn exact_receipts_include_prefix_completion_and_lossless_revision_strings() {
    let manifest = manifest(options(), 0.2, false);
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let decoded: PreparedCaptureManifest = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded, manifest);
    let mut kinds = Vec::new();
    let mut output = Vec::new();
    let sampling = walk_samples(&decoded, &CaptureCancellation::default(), |sample| {
        kinds.push(sample.kind);
        assert_eq!(
            sample.observation.publication.scene_revision().get(),
            u64::MAX - 1
        );
        assert_eq!(
            sample.observation.publication.execution_revision().get(),
            u64::MAX
        );
        if let Some(frame) = sample.frame {
            output.push((frame.pts, frame.source_sample.index()));
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(
        kinds,
        [
            ExportSampleKind::Prefix,
            ExportSampleKind::Prefix,
            ExportSampleKind::Prefix,
            ExportSampleKind::Output,
            ExportSampleKind::Output,
            ExportSampleKind::Output,
            ExportSampleKind::Completion,
        ]
    );
    assert_eq!(output, [(0, 3), (1, 4), (2, 5)]);
    assert_eq!(sampling.frames, 3);
    assert_eq!(sampling.start_time, 0.1001);
    assert_eq!(sampling.end_time, 0.2);
    assert_eq!(sampling.source_end, Some(0.2));
    assert_eq!(audio::soundtrack_grid(sampling).unwrap().frames(), 4_805);
}

#[test]
fn explicit_off_grid_stop_replays_its_exact_completion_and_pads_pcm() {
    let mut options = options();
    options.frame_rate = FrameRate::new(16, 1).unwrap();
    options.start_frame = 2;
    options.stop = ExportStop::EndTime(0.21875);
    let manifest = manifest(options, 1.0, true);
    assert_eq!(manifest.samples.last().unwrap().requested_time, 0.21875);
    let sampling = walk_samples(&manifest, &CaptureCancellation::default(), |_| Ok(())).unwrap();
    assert_eq!(sampling.frames, 2);
    assert_eq!(sampling.start_time, 0.125);
    assert_eq!(sampling.end_time, 0.21875);
    assert_eq!(sampling.source_end, None);
    assert_eq!(sampling.reason, ExportEndReason::RequestedStop);
    let grid = audio::soundtrack_grid(sampling).unwrap();
    assert_eq!(grid.frames(), 6_000);
    assert_eq!(grid.mixed_frames(), 4_500);
}

#[test]
fn terminal_hold_keeps_one_publication_and_missing_or_extra_receipts_fail() {
    let mut options = options();
    options.final_hold_seconds = 0.12;
    let valid = manifest(options, 0.2, false);
    let mut held = 0;
    walk_samples(&valid, &CaptureCancellation::default(), |sample| {
        held += usize::from(sample.frame.is_some_and(|frame| frame.held));
        Ok(())
    })
    .unwrap();
    assert_eq!(held, 4);
    for index in [0, 3, valid.samples.len() - 1] {
        let mut invalid = valid.clone();
        invalid.samples.remove(index);
        assert!(walk_samples(&invalid, &CaptureCancellation::default(), |_| Ok(())).is_err());
    }
    let mut extra = valid.clone();
    extra.samples.push(extra.samples.last().unwrap().clone());
    assert!(walk_samples(&extra, &CaptureCancellation::default(), |_| Ok(())).is_err());
    let mut changed_hold = valid;
    changed_hold
        .samples
        .last_mut()
        .unwrap()
        .publication
        .frame_epoch = "99".into();
    assert!(walk_samples(&changed_hold, &CaptureCancellation::default(), |_| Ok(())).is_err());
}

#[test]
fn immutable_receipts_reject_wrong_grid_endpoint_revisions_and_epoch_reuse() {
    let valid = manifest(options(), 0.2, false);
    for mutation in 0..8 {
        let mut candidate = valid.clone();
        match mutation {
            0 => candidate.samples[1].requested_time += 0.001,
            1 => candidate.samples[1].published_time = 0.0,
            2 => candidate.samples[1].publication.scene_revision = "1".into(),
            3 => candidate.samples[1].publication.execution_revision = "1".into(),
            4 => candidate.samples[1].publication.frame_epoch = "0".into(),
            5 => candidate.samples[1].publication.frame_epoch = "1".into(),
            6 => candidate.samples.last_mut().unwrap().published_time = 0.19,
            7 => candidate.source_end = f64::INFINITY,
            _ => unreachable!(),
        }
        assert!(
            walk_samples(&candidate, &CaptureCancellation::default(), |_| Ok(())).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn revision_strings_reject_lossy_or_ambiguous_encodings() {
    for value in [
        "",
        "01",
        "+1",
        "-1",
        "1.0",
        "1e2",
        " 1",
        "18446744073709551616",
    ] {
        assert!(revision(value).is_err(), "{value}");
    }
    assert_eq!(revision("0").unwrap(), 0);
    assert_eq!(revision("18446744073709551615").unwrap(), u64::MAX);
    let mut value = serde_json::to_value(manifest(options(), 0.2, false)).unwrap();
    value["samples"][0]["publication"]["frame_epoch"] = 1.into();
    assert!(serde_json::from_value::<PreparedCaptureManifest>(value).is_err());
}

#[test]
fn schema_budgets_and_unknown_fields_fail_before_encoding() {
    let root = Temp::new();
    let valid = manifest(options(), 0.2, false);
    write_manifest(&root.0, &valid);
    for limits in [
        PreparedCaptureLimits {
            max_manifest_bytes: 8,
            ..PreparedCaptureLimits::default()
        },
        PreparedCaptureLimits {
            max_samples: 2,
            ..PreparedCaptureLimits::default()
        },
        PreparedCaptureLimits {
            max_frame_bytes: 0,
            ..PreparedCaptureLimits::default()
        },
        PreparedCaptureLimits {
            max_manifest_bytes: u64::MAX,
            ..PreparedCaptureLimits::default()
        },
    ] {
        assert!(read_manifest(&root.0, limits).is_err());
    }
    let mut wrong_version = valid.clone();
    wrong_version.format = "noon-prepared-capture-v2".into();
    write_manifest(&root.0, &wrong_version);
    assert!(read_manifest(&root.0, PreparedCaptureLimits::default()).is_err());
    for location in ["root", "frame_options", "stop", "sample", "publication"] {
        let mut value = serde_json::to_value(&valid).unwrap();
        let target = match location {
            "root" => &mut value,
            "frame_options" => &mut value["frame_options"],
            "stop" => &mut value["frame_options"]["stop"],
            "sample" => &mut value["samples"][0],
            _ => &mut value["samples"][0]["publication"],
        };
        target["unknown"] = true.into();
        assert!(
            serde_json::from_value::<PreparedCaptureManifest>(value).is_err(),
            "{location}"
        );
    }
}

#[test]
fn media_sizes_and_dimension_budgets_fail_before_starting_an_encoder() {
    let root = Temp::new();
    let manifest = manifest(options(), 0.2, true);
    write_media(&root.0, &manifest);
    for limits in [
        PreparedCaptureLimits {
            max_frame_bytes: 63,
            ..PreparedCaptureLimits::default()
        },
        PreparedCaptureLimits {
            max_media_bytes: 100,
            ..PreparedCaptureLimits::default()
        },
    ] {
        let error = export_prepared_capture(
            &root.0,
            missing_encoder(&root.0),
            limits,
            CaptureCancellation::default(),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
    for filename in [FRAMES_FILENAME, PCM_FILENAME] {
        let path = root.0.join(filename);
        let original = fs::read(&path).unwrap();
        for length in [original.len() - 1, original.len() + 1] {
            fs::write(&path, vec![0_u8; length]).unwrap();
            let error = export_prepared_capture(
                &root.0,
                missing_encoder(&root.0),
                PreparedCaptureLimits::default(),
                CaptureCancellation::default(),
            )
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        }
        fs::write(path, original).unwrap();
    }
    assert!(!root.0.join("output.mp4").exists());
    assert_no_staging(&root.0);
}

#[test]
fn unadvertised_audio_is_not_silently_dropped() {
    let root = Temp::new();
    let manifest = manifest(options(), 0.2, false);
    write_media(&root.0, &manifest);
    fs::write(root.0.join(PCM_FILENAME), []).unwrap();
    let error = export_prepared_capture(
        &root.0,
        missing_encoder(&root.0),
        PreparedCaptureLimits::default(),
        CaptureCancellation::default(),
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("unexpected PCM"));
    assert_no_staging(&root.0);
}

#[test]
fn cancelled_import_and_encoder_failure_preserve_destination_and_clean_staging() {
    let root = Temp::new();
    let manifest = manifest(options(), 0.2, false);
    write_media(&root.0, &manifest);
    let mut output = missing_encoder(&root.0);
    output.overwrite = true;
    fs::write(&output.path, b"keep old movie").unwrap();
    let cancellation = CaptureCancellation::default();
    cancellation.cancel();
    let error = export_prepared_capture(
        &root.0,
        output.clone(),
        PreparedCaptureLimits::default(),
        cancellation,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    let error = export_prepared_capture(
        &root.0,
        output.clone(),
        PreparedCaptureLimits::default(),
        CaptureCancellation::default(),
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert_eq!(fs::read(&output.path).unwrap(), b"keep old movie");
    assert_no_staging(&root.0);
}

#[test]
fn receipt_consumer_cancellation_stops_before_the_next_sample() {
    let manifest = manifest(options(), 0.2, false);
    let cancellation = CaptureCancellation::default();
    let mut consumed = 0;
    let error = walk_samples(&manifest, &cancellation, |_| {
        consumed += 1;
        cancellation.cancel();
        Ok(())
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(consumed, 1);
}

fn pcm(root: &Path, samples: &[f32], frames: u64, mixed_frames: u64) -> PreparedPcm {
    let path = root.join("input-pcm");
    fs::write(
        &path,
        samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    PreparedPcm {
        file: File::open(path).unwrap(),
        frames,
        mixed_frames,
    }
}

#[test]
fn pcm_is_copied_exactly_and_rejects_nonfinite_overload_tail_and_extra_bytes() {
    let root = Temp::new();
    let samples = [0.25, -0.25, -1.0, 1.0, 0.0, -0.0];
    let mut output = Vec::new();
    pcm(&root.0, &samples, 3, 2)
        .write_pcm(&mut output, &CaptureCancellation::default())
        .unwrap();
    assert_eq!(
        output,
        samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect::<Vec<_>>()
    );
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.01, -1.01] {
        let mut output = Vec::new();
        let error = pcm(&root.0, &[value, 0.0], 1, 1)
            .write_pcm(&mut output, &CaptureCancellation::default())
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(output.is_empty());
    }
    assert!(pcm(&root.0, &[0.1, 0.0], 1, 0)
        .write_pcm(&mut Vec::new(), &CaptureCancellation::default())
        .is_err());
    assert!(pcm(&root.0, &[0.0; 4], 1, 1)
        .write_pcm(&mut Vec::new(), &CaptureCancellation::default())
        .is_err());
    let error = pcm(&root.0, &[0.0], 1, 1)
        .write_pcm(&mut Vec::new(), &CaptureCancellation::default())
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
}

#[test]
fn pcm_cancellation_is_observed_between_bounded_blocks() {
    struct CancelWriter {
        cancellation: CaptureCancellation,
        bytes: usize,
    }
    impl Write for CancelWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.bytes += buffer.len();
            self.cancellation.cancel();
            Ok(buffer.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let root = Temp::new();
    let frames = MAX_AUDIO_MIX_BLOCK_FRAMES as u64 + 1;
    let cancellation = CaptureCancellation::default();
    let mut writer = CancelWriter {
        cancellation: cancellation.clone(),
        bytes: 0,
    };
    let error = pcm(
        &root.0,
        &vec![0.0; frames as usize * CHANNELS],
        frames,
        frames,
    )
    .write_pcm(&mut writer, &cancellation)
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(writer.bytes, MAX_AUDIO_MIX_BLOCK_FRAMES * CHANNELS * 4);
}
