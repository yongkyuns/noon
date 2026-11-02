//! Direct Rust pair of tests/fixtures/fourier_audio/audio_ordinary_play.py.
//! One ordinary Create(Circle) / wait continuation prepares silently once, then
//! its completed reader and original fixed score are exported without the source.

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use noon::example_scenes::ordinary_audio_play;
    use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
    use noon::PreparedSourceStatus;
    use noon_export::output::{export_completed_prefix_file, OutputOptions};
    use noon_export::CaptureOptions;

    let mut arguments = std::env::args_os().skip(1);
    let output = arguments
        .next()
        .ok_or("usage: render_prepared_audio_video OUTPUT.mp4")?;
    if arguments.next().is_some() {
        return Err("usage: render_prepared_audio_video OUTPUT.mp4".into());
    }
    let (scene, continuation, progress) =
        ordinary_audio_play::scene().map_err(std::io::Error::other)?;
    let mut source = scene.into_prepared_source(continuation, Default::default())?;
    assert_eq!(source.prepare_next()?, PreparedSourceStatus::ReaderReady);
    let mut reader = source.take_reader().expect("initial completed prefix");
    loop {
        match source.prepare_next()? {
            PreparedSourceStatus::BatchReady => {
                let batch = source.pending_batch().expect("prepared suffix");
                reader.reserve_object_capacity(batch.required_object_capacity())?;
                reader.preflight_append(batch)?;
                reader.append(source.take_batch().expect("original suffix"))?;
            }
            PreparedSourceStatus::Finished => break,
            state => return Err(format!("unexpected preparation state {state:?}").into()),
        }
    }
    assert_eq!(progress.calls(), 3);
    assert_eq!(reader.end_time(), Some(3.0));
    assert_eq!(
        reader.frame().time,
        0.0,
        "preparation never selected future history"
    );
    let runtime = reader.runtime_identity();
    drop(source);
    assert_eq!(
        progress.drops(),
        1,
        "source continuation released before export"
    );
    let source_calls_before = progress.calls();
    let source_drops_before_export = progress.drops();
    let summary = export_completed_prefix_file(
        &mut reader,
        ExportFrameOptions {
            frame_rate: FrameRate::new(30_000, 1_001)?,
            start_frame: 0,
            stop: ExportStop::SourceEnd,
            max_frames: 100,
            max_transitions_per_sample: 64,
            final_hold_seconds: 0.0,
        },
        CaptureOptions::new(640, 360),
        OutputOptions::mp4(output),
    )?;
    assert_eq!(reader.runtime_identity(), runtime);
    assert_eq!(progress.calls(), 3);
    assert_eq!(summary.capture.sampling.frames, 90);
    println!(
        "{}",
        serde_json::json!({
            "profile": "completed_prepared_history_native_export",
            "path": summary.path,
            "frames": summary.capture.sampling.frames,
            "width": 640,
            "height": 360,
            "frame_rate": { "numerator": 30_000, "denominator": 1_001 },
            "source_end": reader.end_time(),
            "runtime_before": runtime.get().to_string(),
            "runtime_after": reader.runtime_identity().get().to_string(),
            "source_calls_before": source_calls_before,
            "source_calls_after": progress.calls(),
            "source_drops_before_export": source_drops_before_export,
        })
    );
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}
