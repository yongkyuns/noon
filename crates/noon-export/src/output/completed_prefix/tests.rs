use super::*;

use crate::CaptureCancellation;
use noon::integration::{ExportStop, FrameRate};
use noon::{
    AudioClipOptions, AudioResource, ContinuationStep, LiveContinuation, LiveSession,
    LiveSessionError, PreparedSource, PreparedSourceStatus, Scene,
};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "noon-completed-prefix-export-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
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
struct Source(bool);
impl LiveContinuation for Source {
    type Error = LiveSessionError;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        if self.0 {
            Ok(ContinuationStep::Finished)
        } else {
            self.0 = true;
            live.wait_segment(0.25).map(ContinuationStep::Await)
        }
    }
}
fn fixture() -> (PreparedSource<Source>, PrefixReader) {
    let mut scene = Scene::new();
    let circle = scene.circle(0.5).unwrap();
    scene.add(&circle).unwrap();
    scene
        .audio(
            AudioResource::from_interleaved(8000, 1, vec![0.25; 8000], 32_000).unwrap(),
            AudioClipOptions::default(),
        )
        .unwrap();
    let mut source = scene
        .into_prepared_source(Source(false), Default::default())
        .unwrap();
    assert_eq!(
        source.prepare_next().unwrap(),
        PreparedSourceStatus::ReaderReady
    );
    let reader = source.take_reader().unwrap();
    (source, reader)
}
fn finish(source: &mut PreparedSource<Source>, reader: &mut PrefixReader) {
    assert_eq!(
        source.prepare_next().unwrap(),
        PreparedSourceStatus::BatchReady
    );
    let batch = source.pending_batch().unwrap();
    reader
        .reserve_object_capacity(batch.required_object_capacity())
        .unwrap();
    reader.preflight_append(batch).unwrap();
    reader.append(source.take_batch().unwrap()).unwrap();
    assert_eq!(reader.end_time(), Some(0.25));
}
fn options() -> ExportFrameOptions {
    ExportFrameOptions {
        frame_rate: FrameRate::new(16, 1).unwrap(),
        start_frame: 0,
        stop: ExportStop::SourceEnd,
        max_frames: 16,
        max_transitions_per_sample: 64,
        final_hold_seconds: 0.125,
    }
}

#[test]
fn unfinished_or_nonzero_reader_fails_before_native_allocation_and_destination_changes() {
    let root = Temp::new();
    let path = root.0.join("retained.mp4");
    fs::write(&path, b"existing movie").unwrap();
    let mut output = OutputOptions::mp4(&path);
    output.overwrite = true;
    output.ffmpeg = root.0.join("must-not-run");
    let (mut source, mut reader) = fixture();
    let before = reader.publication_context();
    assert!(matches!(
        export_completed_prefix_file(
            &mut reader,
            options(),
            CaptureOptions::new(64, 48),
            output.clone()
        ),
        Err(CompletedPrefixFileExportError::Sampling(
            CompletedPrefixExportError::Reader(noon::PrefixError::NotReady)
        ))
    ));
    assert_eq!(reader.publication_context(), before);
    finish(&mut source, &mut reader);
    reader.seek(0.125).unwrap();
    assert!(matches!(
        export_completed_prefix_file(&mut reader, options(), CaptureOptions::new(64, 48), output),
        Err(CompletedPrefixFileExportError::Sampling(
            CompletedPrefixExportError::NotAtStart
        ))
    ));
    assert_eq!(fs::read(&path).unwrap(), b"existing movie");
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
}

#[test]
fn cancelled_native_export_preserves_owner_and_destination() {
    let root = Temp::new();
    let (mut source, mut reader) = fixture();
    finish(&mut source, &mut reader);
    let before = reader.publication_context();
    let capture = CaptureOptions::new(64, 48);
    capture.cancellation.cancel();
    let error = export_completed_prefix_file(
        &mut reader,
        options(),
        capture,
        OutputOptions::mp4(root.0.join("cancelled.mp4")),
    )
    .unwrap_err();
    assert!(
        matches!(error, CompletedPrefixFileExportError::Output(ref error) if error.kind() == io::ErrorKind::Interrupted)
    );
    assert_eq!(reader.publication_context(), before);
    assert_eq!(reader.frame().time, 0.0);
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
}

#[test]
fn native_pcm_staging_borrows_original_completed_closure_and_silences_picture_hold() {
    let root = Temp::new();
    let (mut source, mut reader) = fixture();
    finish(&mut source, &mut reader);
    drop(source);
    let context = reader.publication_context();
    let export = CompletedPrefixExport::new(&reader, options()).unwrap();
    let soundtrack = Soundtrack::completed_prefix(&reader, &export).unwrap();
    soundtrack
        .stage(&root.0, &CaptureCancellation::default())
        .unwrap();
    let bytes = fs::read(root.0.join(super::super::audio::PCM_FILENAME)).unwrap();
    let samples = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect::<Vec<_>>();
    assert_eq!(samples.len(), 18_000 * 2);
    assert!(samples[..12_000 * 2].iter().all(|sample| *sample == 0.25));
    assert!(samples[12_000 * 2..].iter().all(|sample| *sample == 0.0));
    assert_eq!(reader.frame().time, 0.0);
    assert_eq!(reader.publication_context(), context);
}
