//! Direct Rust/WASM qualification of completed history and its fixed score.
//! Engine operations and renderer publications stay typed in this WASM context.
//! Only the final evidence record is encoded for the external browser harness.

use noon::example_scenes::ordinary_audio_play;
use noon::integration::{
    CompletedPrefixExport, ExportFrameOptions, ExportFramePolicyStatus, ExportStop, FrameRate,
    MAX_AUDIO_MIX_BLOCK_FRAMES,
};
use noon::PreparedSourceStatus;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct Evidence {
    profile: &'static str,
    source_calls_before: u32,
    source_calls_after: u32,
    source_drops_before_export: u32,
    source_end: f64,
    prepared_time: f64,
    final_time: f64,
    runtime_before: String,
    runtime_after: String,
    frames: u64,
    observations: u64,
    execution_revisions_advanced: bool,
    frame_epochs_advanced: bool,
    final_objects: usize,
    audio_frames: u64,
    silent_padding_frames: u64,
    pcm_peak: f32,
    pcm_energy: f64,
    pcm_checksum: String,
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    condition.then_some(()).ok_or_else(|| message.to_owned())
}

fn qualify() -> Result<Evidence, String> {
    let (scene, continuation, progress) = ordinary_audio_play::scene()?;
    let mut source = scene
        .into_prepared_source(continuation, Default::default())
        .map_err(|e| e.to_string())?;
    require(
        source.prepare_next().map_err(|e| e.to_string())? == PreparedSourceStatus::ReaderReady,
        "ordinary source did not publish its first completed interval",
    )?;
    let mut reader = source
        .take_reader()
        .ok_or("prepared source lost its reader")?;
    loop {
        match source.prepare_next().map_err(|e| e.to_string())? {
            PreparedSourceStatus::BatchReady => {
                let batch = source
                    .pending_batch()
                    .ok_or("prepared source lost its suffix")?;
                reader
                    .reserve_object_capacity(batch.required_object_capacity())
                    .map_err(|e| e.to_string())?;
                reader.preflight_append(batch).map_err(|e| e.to_string())?;
                reader
                    .append(
                        source
                            .take_batch()
                            .ok_or("prepared source lost its original suffix")?,
                    )
                    .map_err(|e| e.to_string())?;
            }
            PreparedSourceStatus::Finished => break,
            state => {
                return Err(format!(
                    "unexpected ordinary source preparation state {state:?}"
                ))
            }
        }
    }
    let source_calls_before = progress.calls();
    let prepared_time = reader.frame().time;
    require(
        source_calls_before == 3 && reader.end_time() == Some(3.0) && prepared_time == 0.0,
        "ordinary audio/Create/wait source did not prepare once at time zero",
    )?;
    let runtime = reader.runtime_identity();
    drop(source);
    let source_drops_before_export = progress.drops();
    require(
        source_drops_before_export == 1,
        "export still retains the source continuation",
    )?;
    let mut export = CompletedPrefixExport::new(
        &reader,
        ExportFrameOptions {
            frame_rate: FrameRate::new(30_000, 1_001).map_err(|e| e.to_string())?,
            start_frame: 0,
            stop: ExportStop::SourceEnd,
            max_frames: 100,
            max_transitions_per_sample: 64,
            final_hold_seconds: 0.0,
        },
    )
    .map_err(|e| e.to_string())?;
    let admission = export.owner();
    let mut frames = 0;
    let mut observations = 0;
    loop {
        match export.advance(&mut reader).map_err(|e| e.to_string())? {
            ExportFramePolicyStatus::SampleReady(sample) => {
                // Consume the ordinary typed publication at this boundary. This
                // checks actual authored rows and receipt identity; it makes no
                // claim about GPU presentation or device audio in the probe.
                let publication = reader.take_renderer_publication();
                require(
                    publication.context() == sample.observation.publication
                        && publication.frame().time == sample.observation.published_time,
                    "export sample is not the actual retained renderer publication",
                )?;
                for (index, object) in publication.frame().objects.iter().enumerate() {
                    require(
                        publication.object_index(object.id) == Some(index),
                        "typed renderer publication lost its physical row association",
                    )?;
                }
                observations += 1;
                frames += u64::from(sample.frame.is_some());
                export
                    .acknowledge_sample(&reader, sample)
                    .map_err(|e| e.to_string())?;
            }
            ExportFramePolicyStatus::Complete(summary) => {
                require(
                    summary == export.planned() && summary.frames == frames,
                    "completed typed export differs from its admitted frame policy",
                )?;
                break;
            }
            ExportFramePolicyStatus::NeedsSample(_) => {
                return Err("export did not settle its next sample".into())
            }
        }
    }
    let grid = export
        .audio_grid()
        .ok_or("ordinary source lost its fixed score")?;
    let channels = usize::from(grid.format().channels());
    let mut first = 0;
    let mut silent_padding_frames = 0;
    let mut peak = 0.0_f32;
    let mut energy = 0.0_f64;
    let mut checksum = 0xcbf29ce484222325_u64;
    let mut output = vec![0.0; MAX_AUDIO_MIX_BLOCK_FRAMES * channels];
    while first < grid.frames() {
        let frames = (grid.frames() - first).min(MAX_AUDIO_MIX_BLOCK_FRAMES as u64) as usize;
        let output = &mut output[..frames * channels];
        export
            .mix_audio_block(&reader, first, output)
            .map_err(|e| e.to_string())?;
        let mixed = grid
            .mixed_frames_in_block(first, frames)
            .map_err(|e| e.to_string())?;
        require(
            output[mixed * channels..]
                .iter()
                .all(|sample| *sample == 0.0),
            "audio padding beyond the completed source is not silent",
        )?;
        silent_padding_frames += (frames - mixed) as u64;
        for sample in output {
            require(
                sample.is_finite(),
                "authored PCM contains a nonfinite sample",
            )?;
            peak = peak.max(sample.abs());
            energy += f64::from(*sample) * f64::from(*sample);
            for byte in sample.to_le_bytes() {
                checksum = (checksum ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        }
        first += frames as u64;
    }
    require(
        progress.calls() == source_calls_before && reader.runtime_identity() == runtime,
        "export reran the source or replaced its adopted reader",
    )?;
    require(
        frames == 90
            && observations == 91
            && reader.frame().time == 3.0
            && peak > 0.0
            && energy > 0.0,
        "typed export did not produce the complete ordinary scene and score",
    )?;
    Ok(Evidence {
        profile: "completed_prepared_history_rust_wasm",
        source_calls_before,
        source_calls_after: progress.calls(),
        source_drops_before_export,
        source_end: export.source_end(),
        prepared_time,
        final_time: reader.frame().time,
        runtime_before: runtime.get().to_string(),
        runtime_after: reader.runtime_identity().get().to_string(),
        frames,
        observations,
        execution_revisions_advanced: reader.publication_context().execution_revision()
            != admission.execution_revision(),
        frame_epochs_advanced: reader.publication_context().frame_epoch()
            != admission.frame_epoch(),
        final_objects: reader.frame().objects.len(),
        audio_frames: grid.frames(),
        silent_padding_frames,
        pcm_peak: peak,
        pcm_energy: energy,
        pcm_checksum: format!("{checksum:016x}"),
    })
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(js_name = verifyCompletedHistoryExport)]
pub fn verify_completed_history_export() -> Result<String, wasm_bindgen::JsValue> {
    let evidence = qualify().map_err(|e| wasm_bindgen::JsValue::from_str(&e))?;
    serde_json::to_string(&evidence).map_err(|e| wasm_bindgen::JsValue::from_str(&e.to_string()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn exact_rust_audio_create_wait_export_runs_typed_probe_without_source_or_runtime_replacement()
    {
        let evidence = super::qualify().unwrap();
        assert_eq!(evidence.frames, 90);
        assert_eq!(evidence.observations, 91);
        assert_eq!(evidence.audio_frames, 144_144);
        assert_eq!(evidence.silent_padding_frames, 144);
        assert_eq!(evidence.source_calls_before, evidence.source_calls_after);
        assert_eq!(evidence.source_drops_before_export, 1);
        assert_eq!(evidence.runtime_before, evidence.runtime_after);
        assert!(!evidence.execution_revisions_advanced);
        assert!(evidence.frame_epochs_advanced);
        assert_eq!(evidence.final_objects, 1);
    }
}
