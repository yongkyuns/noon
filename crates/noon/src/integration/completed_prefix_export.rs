//! Export coordination over one already-adopted deterministic history reader.
//!
//! This owns only output policy and exact owner/receipt checks. The borrowed
//! reader remains the sole historical runtime; its ordinary evaluator, retained
//! publications and immutable soundtrack closure do all execution and mixing.

use std::{error::Error, fmt};

use noon_core::PublicationContext;

use super::{
    AudioMixError, AudioMixFormat, ExportAudioGrid, ExportAudioGridError, ExportFrameOptions,
    ExportFramePolicy, ExportFramePolicyError, ExportFramePolicyStatus, ExportFrameSummary,
    ExportSample, RuntimeIdentity, SampleObservation, MAX_AUDIO_MIX_BLOCK_FRAMES,
};
use crate::{PrefixError, PrefixReader};

/// A completed-history export borrowing its reader separately for each operation.
///
/// Admission requires a terminal batch already adopted at time zero, with no
/// active input effect, selection, hover or inspection adjustment. Source
/// preparation, callback execution and causal replay are outside this capability.
/// The source can already have been dropped; no source continuation is retained
/// or rerun, and no scene/runtime is cloned or reconstructed.
///
/// `advance` settles exactly the shared policy's next coordinate and offers the
/// resulting real publication. Historical revisions legitimately change its
/// execution revision. Between calls the reader must retain the exact expected
/// publication, time and inspection revision; an external seek/input loses this
/// export owner. Consume the publication and complete any render/pixel/sink work
/// before acknowledging the exact sample. Acknowledgement is not a GPU fence.
pub struct CompletedPrefixExport {
    policy: ExportFramePolicy,
    runtime: RuntimeIdentity,
    owner: PublicationContext,
    expected: SampleObservation,
    inspection_revision: u64,
    source_end: f64,
    planned: ExportFrameSummary,
    audio_grid: Option<ExportAudioGrid>,
}

impl CompletedPrefixExport {
    pub fn new(
        reader: &PrefixReader,
        options: ExportFrameOptions,
    ) -> Result<Self, CompletedPrefixExportError> {
        let source_end = reader
            .completed_history_end_time()
            .map_err(CompletedPrefixExportError::Reader)?;
        Self::validate_preparation(reader)?;
        let policy = ExportFramePolicy::new(options)?;
        // Completion intent is bounded even when a requested crop would finish
        // early. This capability admits an already-complete history, not a live
        // producer allowed to run arbitrarily far beyond its export safety cap.
        if source_end > policy.source_preparation_limit()? {
            return Err(ExportFramePolicyError::FrameLimit.into());
        }
        let planned = policy.finite_summary(source_end)?;
        let audio_grid = if reader.has_audio() {
            let grid = ExportAudioGrid::new(planned, AudioMixFormat::STEREO_48_KHZ)?
                .truncate_at(source_end)?;
            // Preflight absolute mixer coordinates before allocating output or
            // publishing a destination. This neither selects time nor emits PCM.
            reader.mix_completed_audio_block(
                grid.scene_start(),
                grid.frames(),
                grid.format(),
                &mut [],
            )?;
            Some(grid)
        } else {
            None
        };
        let owner = reader.publication_context();
        Ok(Self {
            policy,
            runtime: reader.runtime_identity(),
            owner,
            expected: SampleObservation {
                requested_time: 0.0,
                published_time: 0.0,
                publication: owner,
            },
            inspection_revision: reader.inspection_view_revision(),
            source_end,
            planned,
            audio_grid,
        })
    }

    pub const fn source_end(&self) -> f64 {
        self.source_end
    }

    /// Check the existing paused reader view before requesting further source
    /// preparation. This admits neither unfinished history nor file output;
    /// `new` separately requires terminal adoption before sampling can begin.
    pub fn validate_preparation(reader: &PrefixReader) -> Result<(), CompletedPrefixExportError> {
        if reader.frame().time != 0.0 {
            return Err(CompletedPrefixExportError::NotAtStart);
        }
        validate_viewer_state(reader)
    }

    pub const fn planned(&self) -> ExportFrameSummary {
        self.planned
    }

    pub const fn audio_grid(&self) -> Option<ExportAudioGrid> {
        self.audio_grid
    }

    /// Fixed admission context, distinct from later historical publications.
    pub const fn owner(&self) -> PublicationContext {
        self.owner
    }

    fn validate_owner(&self, reader: &PrefixReader) -> Result<(), CompletedPrefixExportError> {
        if reader.runtime_identity() != self.runtime
            || reader.completed_history_end_time().ok() != Some(self.source_end)
            || reader.publication_context() != self.expected.publication
            || reader.frame().time != self.expected.published_time
            || reader.inspection_view_revision() != self.inspection_revision
        {
            return Err(CompletedPrefixExportError::OwnerChanged);
        }
        validate_viewer_state(reader)
    }

    pub fn status(
        &self,
        reader: &PrefixReader,
    ) -> Result<ExportFramePolicyStatus, CompletedPrefixExportError> {
        self.validate_owner(reader)?;
        let status = self.policy.status()?;
        if matches!(status, ExportFramePolicyStatus::Complete(summary) if summary != self.planned) {
            return Err(CompletedPrefixExportError::OwnerChanged);
        }
        Ok(status)
    }

    /// Settle one policy-selected sample. A pending sample is returned unchanged;
    /// repeated polling cannot advance history or skip a consumer barrier.
    pub fn advance(
        &mut self,
        reader: &mut PrefixReader,
    ) -> Result<ExportFramePolicyStatus, CompletedPrefixExportError> {
        let result = self.advance_inner(reader);
        if result.is_err() {
            self.cancel();
        }
        result
    }

    fn advance_inner(
        &mut self,
        reader: &mut PrefixReader,
    ) -> Result<ExportFramePolicyStatus, CompletedPrefixExportError> {
        let ExportFramePolicyStatus::NeedsSample(requested_time) = self.status(reader)? else {
            return self.status(reader);
        };
        let target = requested_time.min(self.source_end);
        if reader.frame().time != target {
            reader
                .advance_to(target)
                .map_err(CompletedPrefixExportError::Reader)?;
        }
        let observation = SampleObservation {
            requested_time,
            published_time: reader.frame().time,
            publication: reader.publication_context(),
        };
        let before = self.expected;
        let previous = before.publication;
        let current = observation.publication;
        if observation.published_time != target
            || current.scene_revision() != self.owner.scene_revision()
            || current.execution_revision().get() < previous.execution_revision().get()
            || current.frame_epoch().get() < previous.frame_epoch().get()
            || current.execution_revision().get() - previous.execution_revision().get()
                > current.frame_epoch().get() - previous.frame_epoch().get()
            || (observation.published_time != before.published_time
                && current.frame_epoch() == previous.frame_epoch())
        {
            return Err(CompletedPrefixExportError::OwnerChanged);
        }
        self.policy
            .observe(observation, requested_time >= self.source_end)?;
        self.expected = observation;
        self.status(reader)
    }

    /// Accept only the exact offered publication after its external consumer
    /// completes. A wrong receipt changes no policy state, allowing correction.
    pub fn acknowledge_sample(
        &mut self,
        reader: &PrefixReader,
        sample: ExportSample,
    ) -> Result<(), CompletedPrefixExportError> {
        self.validate_owner(reader)?;
        self.policy.acknowledge_sample(sample)?;
        Ok(())
    }

    /// Mix bounded authored PCM using the reader's original immutable closure.
    /// Crop phase is absolute; completed-source padding and terminal holds are
    /// silence. This is read-only with respect to history, clocks and publication.
    /// Errors leave the caller's output unchanged.
    pub fn mix_audio_block(
        &self,
        reader: &PrefixReader,
        first_frame: u64,
        output: &mut [f32],
    ) -> Result<(), CompletedPrefixExportError> {
        self.validate_owner(reader)?;
        self.policy.status()?;
        let grid = self.audio_grid.ok_or(CompletedPrefixExportError::NoAudio)?;
        let channels = usize::from(grid.format().channels());
        if !output.len().is_multiple_of(channels)
            || output.is_empty()
            || output.len() / channels > MAX_AUDIO_MIX_BLOCK_FRAMES
        {
            return Err(CompletedPrefixExportError::InvalidAudioBlock);
        }
        let mixed = grid.mixed_frames_in_block(first_frame, output.len() / channels)?;
        if mixed > 0 {
            reader.mix_completed_audio_block(
                grid.scene_start(),
                first_frame,
                grid.format(),
                &mut output[..mixed * channels],
            )?;
        }
        output[mixed * channels..].fill(0.0);
        Ok(())
    }

    pub fn cancel(&mut self) {
        self.policy.cancel();
    }
}

fn validate_viewer_state(reader: &PrefixReader) -> Result<(), CompletedPrefixExportError> {
    let authored = reader
        .camera_2d()
        .map_err(|_| CompletedPrefixExportError::ViewerState)?
        .unwrap_or_default();
    let displayed = reader
        .inspection_camera()
        .map_err(|_| CompletedPrefixExportError::ViewerState)?;
    if reader.interactions_active()
        || reader.pointer_gesture_active()
        || reader.selected_pointer_target().is_some()
        || reader.hovered_pointer_target().is_some()
        || displayed != authored
    {
        return Err(CompletedPrefixExportError::ViewerState);
    }
    Ok(())
}

#[derive(Debug)]
pub enum CompletedPrefixExportError {
    NotAtStart,
    ViewerState,
    OwnerChanged,
    NoAudio,
    InvalidAudioBlock,
    Reader(PrefixError),
    Sampling(ExportFramePolicyError),
    AudioGrid(ExportAudioGridError),
    AudioMix(AudioMixError),
}

impl fmt::Display for CompletedPrefixExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAtStart => formatter.write_str("completed-history export requires time zero"),
            Self::ViewerState => formatter.write_str("completed-history export requires an unmodified authored view without active input"),
            Self::OwnerChanged => formatter.write_str("completed-history export lost its admitted reader or exact publication"),
            Self::NoAudio => formatter.write_str("completed-history export has no authored audio"),
            Self::InvalidAudioBlock => formatter.write_str("completed-history audio block exceeds its bounded interleaved profile"),
            Self::Reader(error) => error.fmt(formatter),
            Self::Sampling(error) => error.fmt(formatter),
            Self::AudioGrid(error) => error.fmt(formatter),
            Self::AudioMix(error) => error.fmt(formatter),
        }
    }
}

impl Error for CompletedPrefixExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Reader(error) => Some(error),
            Self::Sampling(error) => Some(error),
            Self::AudioGrid(error) => Some(error),
            Self::AudioMix(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ExportFramePolicyError> for CompletedPrefixExportError {
    fn from(error: ExportFramePolicyError) -> Self {
        Self::Sampling(error)
    }
}
impl From<ExportAudioGridError> for CompletedPrefixExportError {
    fn from(error: ExportAudioGridError) -> Self {
        Self::AudioGrid(error)
    }
}
impl From<AudioMixError> for CompletedPrefixExportError {
    fn from(error: AudioMixError) -> Self {
        Self::AudioMix(error)
    }
}
