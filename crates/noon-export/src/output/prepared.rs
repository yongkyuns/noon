//! Explicit media import for an execution owner outside the native process.
//!
//! This format carries completed pixels, mixed PCM and publication observations,
//! never a scene, an executable projection, callbacks or a replacement runtime.

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use noon::integration::{
    ExportEndReason, ExportFrameOptions, ExportFramePolicy, ExportFramePolicyStatus,
    ExportFrameSummary, ExportSample, ExportStop, FrameRate, SampleObservation,
    MAX_AUDIO_MIX_BLOCK_FRAMES,
};
use noon_core::{ExecutionRevision, FrameEpoch, PublicationContext, SceneRevision};
use serde::{Deserialize, Serialize};

use super::audio::{self, StagedSoundtrack, CHANNELS, PCM_FILENAME};
use super::{check_publication_cancellation, FileSink, OutputFormat, OutputOptions};
use crate::{CaptureCancellation, CapturePixelFormat, CaptureWork, CapturedFrame};

const MANIFEST_FILENAME: &str = "capture.json";
const FRAMES_FILENAME: &str = "frames.rgba";

/// Versioned media metadata at the explicit external export boundary.
///
/// `frames.rgba` contains tightly packed, top-down opaque RGBA8 frames selected
/// by the shared [`ExportFramePolicy`], in rebased PTS order. Prefix and completion
/// observations are included in `samples` but have no pixels in that file.
/// When `audio` is true, `audio.f32le` contains 48 kHz stereo interleaved PCM from
/// the shared mixer, starting at the policy's absolute cropped scene origin.
/// Its frame count is the integer ceiling of the complete video grid at 48 kHz;
/// samples at or after the policy's exclusive end must be zero.
///
/// The producer must complete renderer consumption before recording a sample,
/// and close and leave all three files immutable before importing. Observations
/// are validated against the shared frame policy; metadata alone cannot prove
/// that an arbitrary producer rendered the supplied pixels or mixed the PCM.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedCaptureManifest {
    pub format: String,
    pub width: u32,
    pub height: u32,
    /// Finite endpoint admitted by the original execution owner.
    pub source_end: f64,
    pub frame_options: PreparedFrameOptions,
    pub audio: bool,
    /// Every observation, including the discarded prefix and final completion.
    pub samples: Vec<PreparedSample>,
}

impl PreparedCaptureManifest {
    pub const FORMAT: &'static str = "noon-prepared-capture-v1";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedFrameRate {
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreparedStop {
    SourceEnd {},
    FrameCount { frames: u64 },
    EndTime { time: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedFrameOptions {
    pub frame_rate: PreparedFrameRate,
    pub start_frame: u64,
    pub stop: PreparedStop,
    pub max_frames: u64,
    pub max_transitions_per_sample: u32,
    pub final_hold_seconds: f64,
}

impl From<ExportFrameOptions> for PreparedFrameOptions {
    fn from(options: ExportFrameOptions) -> Self {
        Self {
            frame_rate: PreparedFrameRate {
                numerator: options.frame_rate.numerator(),
                denominator: options.frame_rate.denominator(),
            },
            start_frame: options.start_frame,
            stop: match options.stop {
                ExportStop::SourceEnd => PreparedStop::SourceEnd {},
                ExportStop::FrameCount(frames) => PreparedStop::FrameCount { frames },
                ExportStop::EndTime(time) => PreparedStop::EndTime { time },
            },
            max_frames: options.max_frames,
            max_transitions_per_sample: options.max_transitions_per_sample,
            final_hold_seconds: options.final_hold_seconds,
        }
    }
}

impl TryFrom<PreparedFrameOptions> for ExportFrameOptions {
    type Error = io::Error;

    fn try_from(options: PreparedFrameOptions) -> Result<Self, Self::Error> {
        Ok(Self {
            frame_rate: FrameRate::new(
                options.frame_rate.numerator,
                options.frame_rate.denominator,
            )
            .map_err(invalid)?,
            start_frame: options.start_frame,
            stop: match options.stop {
                PreparedStop::SourceEnd {} => ExportStop::SourceEnd,
                PreparedStop::FrameCount { frames } => ExportStop::FrameCount(frames),
                PreparedStop::EndTime { time } => ExportStop::EndTime(time),
            },
            max_frames: options.max_frames,
            max_transitions_per_sample: options.max_transitions_per_sample,
            final_hold_seconds: options.final_hold_seconds,
        })
    }
}

/// Decimal strings preserve all 64 revision bits across a JavaScript boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedPublication {
    pub scene_revision: String,
    pub execution_revision: String,
    pub frame_epoch: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedSample {
    pub requested_time: f64,
    pub published_time: f64,
    pub publication: PreparedPublication,
}

impl From<SampleObservation> for PreparedSample {
    fn from(sample: SampleObservation) -> Self {
        Self {
            requested_time: sample.requested_time,
            published_time: sample.published_time,
            publication: PreparedPublication {
                scene_revision: sample.publication.scene_revision().get().to_string(),
                execution_revision: sample.publication.execution_revision().get().to_string(),
                frame_epoch: sample.publication.frame_epoch().get().to_string(),
            },
        }
    }
}

impl PreparedSample {
    fn observation(&self) -> io::Result<SampleObservation> {
        Ok(SampleObservation {
            requested_time: self.requested_time,
            published_time: self.published_time,
            publication: PublicationContext::new(
                SceneRevision::new(revision(&self.publication.scene_revision)?),
                ExecutionRevision::new(revision(&self.publication.execution_revision)?),
                FrameEpoch::new(revision(&self.publication.frame_epoch)?),
            ),
        })
    }
}

/// Independent admission budgets for received files. Encoding retains one
/// input frame plus the existing sink's bounded buffers and one PCM block.
#[derive(Clone, Copy, Debug)]
pub struct PreparedCaptureLimits {
    pub max_manifest_bytes: u64,
    pub max_samples: usize,
    pub max_frame_bytes: u64,
    /// Sum of the exact RGBA and PCM file lengths, excluding the manifest.
    pub max_media_bytes: u64,
}

impl Default for PreparedCaptureLimits {
    fn default() -> Self {
        Self {
            max_manifest_bytes: 16 * 1024 * 1024,
            max_samples: 1_000_000,
            max_frame_bytes: 256 * 1024 * 1024,
            max_media_bytes: 64 * 1024 * 1024 * 1024,
        }
    }
}

/// A finalized imported movie. This certifies native encoding/publication;
/// it makes no claim that the native process rendered or evaluated the source.
#[derive(Debug)]
pub struct PreparedCaptureSummary {
    pub sampling: ExportFrameSummary,
    pub width: u32,
    pub height: u32,
    pub audio_frames: Option<u64>,
    pub path: PathBuf,
    pub format: OutputFormat,
    pub encoder_diagnostics: String,
}

/// Encode a completed, finite external capture with the existing native MP4 sink.
///
/// The producer remains the sole execution/rendering owner. This consumer checks
/// every receipt with [`ExportFramePolicy`], reads bounded media blocks, and feeds
/// the same encoder and terminal-duration transaction as `export_session_file`.
/// It does not load Python, instantiate a runtime, re-mix PCM or postprocess the
/// encoded soundtrack. Only a fully finalized movie is published; errors and
/// cancellation preserve an existing destination and discard private staging.
pub fn export_prepared_capture(
    directory: impl AsRef<Path>,
    output: OutputOptions,
    limits: PreparedCaptureLimits,
    cancellation: CaptureCancellation,
) -> io::Result<PreparedCaptureSummary> {
    check_publication_cancellation(&cancellation)?;
    if output.format != OutputFormat::Mp4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "prepared capture import requires MP4 output",
        ));
    }
    let directory = directory.as_ref();
    let manifest = read_manifest(directory, limits)?;
    let options = ExportFrameOptions::try_from(manifest.frame_options)?;
    output.validate(options.frame_rate, manifest.width, manifest.height)?;
    let sampling = walk_samples(&manifest, &cancellation, |_| Ok(()))?;
    let frame_bytes = u64::from(manifest.width)
        .checked_mul(u64::from(manifest.height))
        .and_then(|bytes| bytes.checked_mul(4))
        .filter(|&bytes| bytes <= limits.max_frame_bytes)
        .ok_or_else(|| invalid("prepared frame exceeds its byte budget"))?;
    let frame_size = usize::try_from(frame_bytes)
        .ok()
        .filter(|&bytes| bytes <= isize::MAX as usize)
        .ok_or_else(|| invalid("prepared frame allocation is not representable"))?;
    let rgba_bytes = frame_bytes
        .checked_mul(sampling.frames)
        .ok_or_else(|| invalid("prepared frame file length overflows"))?;
    let audio_grid = manifest
        .audio
        .then(|| audio::soundtrack_grid(sampling))
        .transpose()?;
    let audio_frames = audio_grid.map(|grid| grid.frames());
    let pcm_bytes = audio_grid.map_or(0, |grid| grid.byte_len());
    rgba_bytes
        .checked_add(pcm_bytes)
        .filter(|&bytes| bytes <= limits.max_media_bytes)
        .ok_or_else(|| invalid("prepared media exceeds its byte budget"))?;
    let mut frames = exact_file(&directory.join(FRAMES_FILENAME), rgba_bytes)?;
    let mut soundtrack = if let Some(grid) = audio_grid {
        Some(PreparedPcm {
            file: exact_file(&directory.join(PCM_FILENAME), pcm_bytes)?,
            frames: grid.frames(),
            mixed_frames: grid.mixed_frames(),
        })
    } else {
        match fs::symlink_metadata(directory.join(PCM_FILENAME)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
            Ok(_) => return Err(invalid("unexpected PCM file for a capture without audio")),
        }
        None
    };
    let mut pixels = vec![0_u8; frame_size];
    let mut sink = FileSink::with_staged_soundtrack(
        output,
        options.frame_rate,
        manifest.width,
        manifest.height,
        cancellation.clone(),
        soundtrack
            .as_mut()
            .map(|source| source as &mut dyn StagedSoundtrack),
    )?;
    walk_samples(&manifest, &cancellation, |sample| {
        if let Some(frame) = sample.frame {
            frames.read_exact(&mut pixels)?;
            sink.write(CapturedFrame {
                frame,
                observation: sample.observation,
                width: manifest.width,
                height: manifest.height,
                format: CapturePixelFormat::RendererRgba8UnormOpaque,
                rgba: &pixels,
                // Import performs no native renderer work. The private sink
                // does not use this field, and the public summary is distinct.
                work: CaptureWork::default(),
            })?;
        }
        Ok(())
    })?;
    ensure_eof(&mut frames)?;
    check_publication_cancellation(&cancellation)?;
    let source_duration =
        if options.final_hold_seconds == 0.0 && sampling.reason == ExportEndReason::SourceEnd {
            sampling.source_end.map(|end| end - sampling.start_time)
        } else {
            None
        };
    let (path, format, encoder_diagnostics) =
        sink.finish_with_source_end(sampling.frames, source_duration)?;
    Ok(PreparedCaptureSummary {
        sampling,
        width: manifest.width,
        height: manifest.height,
        audio_frames,
        path,
        format,
        encoder_diagnostics,
    })
}

fn read_manifest(
    directory: &Path,
    limits: PreparedCaptureLimits,
) -> io::Result<PreparedCaptureManifest> {
    let read_limit = limits
        .max_manifest_bytes
        .checked_add(1)
        .filter(|&limit| limit <= isize::MAX as u64)
        .ok_or_else(|| invalid("invalid manifest byte budget"))?;
    if limits.max_manifest_bytes == 0
        || limits.max_samples == 0
        || limits.max_frame_bytes == 0
        || limits.max_media_bytes == 0
    {
        return Err(invalid("prepared capture budgets must be positive"));
    }
    let file = regular_file(&directory.join(MANIFEST_FILENAME))?;
    if file.metadata()?.len() > limits.max_manifest_bytes {
        return Err(invalid("prepared manifest exceeds its byte budget"));
    }
    let mut bytes = Vec::new();
    file.take(read_limit).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limits.max_manifest_bytes {
        return Err(invalid("prepared manifest exceeds its byte budget"));
    }
    let manifest: PreparedCaptureManifest = serde_json::from_slice(&bytes).map_err(invalid)?;
    if manifest.format != PreparedCaptureManifest::FORMAT {
        return Err(invalid("unsupported prepared capture format"));
    }
    if manifest.samples.is_empty() || manifest.samples.len() > limits.max_samples {
        return Err(invalid(
            "prepared observation count exceeds its budget or is empty",
        ));
    }
    Ok(manifest)
}

/// Reconcile metadata using the shared policy; this is not source execution or
/// a substitute for the producer's actual renderer-completion acknowledgements.
fn walk_samples(
    manifest: &PreparedCaptureManifest,
    cancellation: &CaptureCancellation,
    mut consume: impl FnMut(ExportSample) -> io::Result<()>,
) -> io::Result<ExportFrameSummary> {
    let mut policy = ExportFramePolicy::new(manifest.frame_options.try_into()?).map_err(invalid)?;
    let planned = policy
        .finite_summary(manifest.source_end)
        .map_err(invalid)?;
    let mut samples = manifest.samples.iter();
    let mut previous: Option<SampleObservation> = None;
    loop {
        check_publication_cancellation(cancellation)?;
        match policy.status().map_err(invalid)? {
            ExportFramePolicyStatus::NeedsSample(time) => {
                let sample = samples
                    .next()
                    .ok_or_else(|| invalid("prepared capture is missing a required observation"))?
                    .observation()?;
                if sample.requested_time != time
                    || sample.published_time != time.min(manifest.source_end)
                    || previous.is_some_and(|previous| {
                        sample.publication.scene_revision() != previous.publication.scene_revision()
                            || sample.publication.execution_revision()
                                != previous.publication.execution_revision()
                            || sample.publication.frame_epoch().get()
                                < previous.publication.frame_epoch().get()
                            || (sample.published_time != previous.published_time
                                && sample.publication.frame_epoch()
                                    == previous.publication.frame_epoch())
                    })
                {
                    return Err(invalid(
                        "prepared observation disagrees with the immutable source or frame grid",
                    ));
                }
                policy
                    .observe(sample, time >= manifest.source_end)
                    .map_err(invalid)?;
                previous = Some(sample);
            }
            ExportFramePolicyStatus::SampleReady(sample) => {
                consume(sample)?;
                policy.acknowledge_sample(sample).map_err(invalid)?;
            }
            ExportFramePolicyStatus::Complete(summary) => {
                if samples.next().is_some() || summary != planned {
                    return Err(invalid(
                        "prepared observations disagree with the complete finite output range",
                    ));
                }
                return Ok(summary);
            }
        }
    }
}

fn revision(value: &str) -> io::Result<u64> {
    if value.is_empty()
        || value.len() > 20
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid(
            "publication revisions must be canonical decimal u64 strings",
        ));
    }
    value.parse().map_err(invalid)
}

fn regular_file(path: &Path) -> io::Result<File> {
    if !fs::metadata(path)?.is_file() {
        return Err(invalid("prepared input must be a regular file"));
    }
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("prepared input must be a regular file"));
    }
    Ok(file)
}

fn exact_file(path: &Path, length: u64) -> io::Result<File> {
    let file = regular_file(path)?;
    if file.metadata()?.len() != length {
        return Err(invalid(format!(
            "{} has an incorrect byte count",
            path.display()
        )));
    }
    Ok(file)
}

fn ensure_eof(file: &mut impl Read) -> io::Result<()> {
    if file.read(&mut [0_u8])? != 0 {
        return Err(invalid("prepared input has extra bytes"));
    }
    Ok(())
}

struct PreparedPcm {
    file: File,
    frames: u64,
    mixed_frames: u64,
}

impl StagedSoundtrack for PreparedPcm {
    fn frames(&self) -> u64 {
        self.frames
    }

    fn stage(&mut self, directory: &Path, cancellation: &CaptureCancellation) -> io::Result<()> {
        check_publication_cancellation(cancellation)?;
        let mut output = BufWriter::new(File::create(directory.join(PCM_FILENAME))?);
        self.write_pcm(&mut output, cancellation)
    }
}

impl PreparedPcm {
    fn write_pcm(
        &mut self,
        output: &mut impl Write,
        cancellation: &CaptureCancellation,
    ) -> io::Result<()> {
        check_publication_cancellation(cancellation)?;
        let mut source = BufReader::new(&mut self.file);
        let mut bytes = vec![0_u8; MAX_AUDIO_MIX_BLOCK_FRAMES * CHANNELS * 4];
        let mut first = 0;
        while first < self.frames {
            check_publication_cancellation(cancellation)?;
            let frames = (self.frames - first).min(MAX_AUDIO_MIX_BLOCK_FRAMES as u64) as usize;
            let block = &mut bytes[..frames * CHANNELS * 4];
            source.read_exact(block)?;
            for (index, bytes) in block.as_chunks::<4>().0.iter().enumerate() {
                let sample = f32::from_le_bytes(*bytes);
                if !sample.is_finite()
                    || !(-1.0..=1.0).contains(&sample)
                    || (first + (index / CHANNELS) as u64 >= self.mixed_frames && sample != 0.0)
                {
                    return Err(invalid(
                        "prepared PCM is nonfinite, overloaded or nonzero beyond the requested end",
                    ));
                }
            }
            output.write_all(block)?;
            first += frames as u64;
        }
        ensure_eof(&mut source)?;
        output.flush()?;
        check_publication_cancellation(cancellation)
    }
}

fn invalid(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests;
