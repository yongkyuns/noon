//! Authored audio clip values and their stateless scene-time interpretation.
//!
//! These values describe a finite clip. They do not own a clock, scheduler,
//! decoder, mixer, or output device; a runtime can sample the same definition
//! after a seek, during playback, or for offline rendering.

use crate::{AudioResource, AudioResourceHandle};

// Frame indices returned as f64 must preserve every integer in the source.
const MAX_EXACT_FRAME_COUNT: u128 = 1 << 53;

/// Inputs for a finite audio clip, with all times expressed in scene seconds.
///
/// The source range is `[source_start_frame, source_end_frame)`, in PCM frames
/// rather than interleaved samples. A one-shot clip defaults to the duration of
/// that range. A loop repeats the whole range and requires an explicit duration.
/// Fades are linear, apply once over the clip, and may touch but not overlap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioClipOptions {
    pub start_time: f64,
    pub source_start_frame: usize,
    pub source_end_frame: Option<usize>,
    pub duration: Option<f64>,
    pub looped: bool,
    pub gain: f64,
    pub fade_in: f64,
    pub fade_out: f64,
}

impl Default for AudioClipOptions {
    fn default() -> Self {
        Self {
            start_time: 0.0,
            source_start_frame: 0,
            source_end_frame: None,
            duration: None,
            looped: false,
            gain: 1.0,
            fade_in: 0.0,
            fade_out: 0.0,
        }
    }
}

/// A validated clip value owned by a scene-global semantic audio node.
///
/// Source metadata is retained so store admission can check that the resource
/// resolved by the handle matches the source used to validate the definition.
/// No PCM is retained or inspected by this value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioClipDefinition {
    resource: AudioResourceHandle,
    sample_rate: u32,
    channels: u16,
    resource_frames: usize,
    source_start_frame: usize,
    source_end_frame: usize,
    start_time: f64,
    end_time: f64,
    duration: f64,
    looped: bool,
    gain: f64,
    fade_in: f64,
    fade_out: f64,
}

impl AudioClipDefinition {
    /// Validate a clip before it enters a semantic transaction.
    ///
    /// The owning store must separately resolve `resource` and call
    /// [`Self::validate_resource`] to establish handle provenance. Construction
    /// does not admit a resource or mutate the scene.
    pub fn new(
        resource: AudioResourceHandle,
        source: &AudioResource,
        options: AudioClipOptions,
    ) -> Result<Self, AudioClipError> {
        if !options.start_time.is_finite() || options.start_time < 0.0 {
            return Err(AudioClipError::InvalidStartTime);
        }
        let resource_frames = source.frames();
        if resource_frames as u128 > MAX_EXACT_FRAME_COUNT {
            return Err(AudioClipError::SourceFrameCountTooLarge);
        }
        let source_end_frame = options.source_end_frame.unwrap_or(resource_frames);
        if options.source_start_frame >= source_end_frame || source_end_frame > resource_frames {
            return Err(AudioClipError::InvalidSourceRange {
                start: options.source_start_frame,
                end: source_end_frame,
                frames: resource_frames,
            });
        }
        let source_duration = (source_end_frame - options.source_start_frame) as f64
            / f64::from(source.sample_rate());
        let duration = match options.duration {
            Some(duration) => duration,
            None if options.looped => return Err(AudioClipError::LoopRequiresDuration),
            None => source_duration,
        };
        if !duration.is_finite() || duration <= 0.0 {
            return Err(AudioClipError::InvalidDuration);
        }
        if !options.looped && duration > source_duration {
            return Err(AudioClipError::DurationExceedsSource);
        }
        let end_time = options.start_time + duration;
        if !end_time.is_finite() || end_time <= options.start_time {
            return Err(AudioClipError::UnrepresentableTimeRange);
        }
        if !options.gain.is_finite() || options.gain < 0.0 {
            return Err(AudioClipError::InvalidGain);
        }
        if !options.fade_in.is_finite() || options.fade_in < 0.0 {
            return Err(AudioClipError::InvalidFadeIn);
        }
        if !options.fade_out.is_finite() || options.fade_out < 0.0 {
            return Err(AudioClipError::InvalidFadeOut);
        }
        if options.fade_out > duration || options.fade_in > duration - options.fade_out {
            return Err(AudioClipError::FadesExceedDuration);
        }
        Ok(Self {
            resource,
            sample_rate: source.sample_rate(),
            channels: source.channels(),
            resource_frames,
            source_start_frame: options.source_start_frame,
            source_end_frame,
            start_time: options.start_time,
            end_time,
            duration,
            looped: options.looped,
            gain: options.gain,
            fade_in: options.fade_in,
            fade_out: options.fade_out,
        })
    }

    pub const fn resource(&self) -> AudioResourceHandle {
        self.resource
    }

    pub const fn start_time(&self) -> f64 {
        self.start_time
    }

    pub const fn end_time(&self) -> f64 {
        self.end_time
    }

    pub const fn duration(&self) -> f64 {
        self.duration
    }

    pub const fn source_start_frame(&self) -> usize {
        self.source_start_frame
    }

    pub const fn source_end_frame(&self) -> usize {
        self.source_end_frame
    }

    pub const fn is_looped(&self) -> bool {
        self.looped
    }

    pub const fn gain(&self) -> f64 {
        self.gain
    }

    pub const fn fade_in(&self) -> f64 {
        self.fade_in
    }

    pub const fn fade_out(&self) -> f64 {
        self.fade_out
    }

    /// Check the resolved source's format and range without scanning its PCM.
    /// Handle provenance and lifetime remain the owning store's responsibility.
    pub fn validate_resource(&self, source: &AudioResource) -> Result<(), AudioClipError> {
        if source.sample_rate() != self.sample_rate
            || source.channels() != self.channels
            || source.frames() != self.resource_frames
        {
            return Err(AudioClipError::ResourceMismatch);
        }
        Ok(())
    }

    pub(crate) fn remap_resource(&mut self, resource: AudioResourceHandle) {
        self.resource = resource;
    }

    /// Resolve source position and gain at one nonnegative finite scene time.
    ///
    /// The active interval is `[start_time(), end_time())`. Source position uses
    /// fractional PCM-frame units; no resampling or sample interpolation is
    /// performed here. Loop phase comes from elapsed PCM frames, so querying a
    /// large jump or repeating a paused time costs the same as a nearby query.
    /// All arithmetic is bounded; neither previous queries nor an audio clock
    /// influence the result.
    pub fn state_at(&self, scene_time: f64) -> Result<Option<AudioClipPosition>, AudioClipError> {
        if !scene_time.is_finite() || scene_time < 0.0 {
            return Err(AudioClipError::InvalidSceneTime);
        }
        if scene_time < self.start_time || scene_time >= self.end_time {
            return Ok(None);
        }
        let elapsed = scene_time - self.start_time;
        let frame_count = self.source_end_frame - self.source_start_frame;
        let frame_offset = if self.looped {
            loop_frame_offset(elapsed, self.sample_rate, frame_count)
        } else {
            elapsed * f64::from(self.sample_rate)
        };
        // Floating addition/multiplication may round an interior position to the
        // exclusive end. Bound only the returned coordinate to its predecessor;
        // scene-time admission and loop boundaries use no tolerance or epsilon.
        let source_frame = (self.source_start_frame as f64 + frame_offset)
            .min((self.source_end_frame as f64).next_down());
        let mut envelope = 1.0;
        if elapsed < self.fade_in {
            envelope = elapsed / self.fade_in;
        }
        let remaining = (self.duration - elapsed).max(0.0);
        if remaining < self.fade_out {
            envelope = envelope.min(remaining / self.fade_out);
        }
        Ok(Some(AudioClipPosition {
            source_frame,
            gain: self.gain * envelope,
        }))
    }
}

fn loop_frame_offset(elapsed: f64, sample_rate: u32, frame_count: usize) -> f64 {
    let rate = f64::from(sample_rate);
    let frames = frame_count as f64;
    if elapsed <= MAX_EXACT_FRAME_COUNT as f64 / rate {
        return (elapsed * rate) % frames;
    }
    // At large times, multiplying first can discard low frame bits or overflow.
    // For integer seconds s, (s * rate) mod frames equals
    // ((s mod frames) * rate) mod frames. Both operands of the first remainder
    // are exact integers, and its result fits in 53 bits; the bounded product
    // then fits in u128. The fractional second contributes at most one rate.
    // This performs fixed work even when the query skips an enormous loop count.
    let seconds_remainder = (elapsed.trunc() % frames) as u128;
    let whole_frames = (seconds_remainder * u128::from(sample_rate)) % frame_count as u128;
    (whole_frames as f64 + elapsed.fract() * rate) % frames
}

/// Position within the original resource, before resampling or interpolation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioClipPosition {
    pub source_frame: f64,
    pub gain: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioClipError {
    InvalidStartTime,
    SourceFrameCountTooLarge,
    InvalidSourceRange {
        start: usize,
        end: usize,
        frames: usize,
    },
    LoopRequiresDuration,
    InvalidDuration,
    DurationExceedsSource,
    UnrepresentableTimeRange,
    InvalidGain,
    InvalidFadeIn,
    InvalidFadeOut,
    FadesExceedDuration,
    ResourceMismatch,
    InvalidSceneTime,
}

impl std::fmt::Display for AudioClipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidStartTime => {
                f.write_str("audio clip start must be finite and nonnegative")
            }
            Self::SourceFrameCountTooLarge => {
                f.write_str("audio source exceeds the exact f64 PCM-frame index range")
            }
            Self::InvalidSourceRange { start, end, frames } => write!(
                f,
                "audio source range [{start}, {end}) must be nonempty and within {frames} frames"
            ),
            Self::LoopRequiresDuration => {
                f.write_str("looping audio requires an explicit duration")
            }
            Self::InvalidDuration => f.write_str("audio clip duration must be finite and positive"),
            Self::DurationExceedsSource => {
                f.write_str("one-shot audio duration exceeds its trimmed source")
            }
            Self::UnrepresentableTimeRange => {
                f.write_str("audio clip end must be finite and representably later than its start")
            }
            Self::InvalidGain => f.write_str("audio clip gain must be finite and nonnegative"),
            Self::InvalidFadeIn => f.write_str("audio fade-in must be finite and nonnegative"),
            Self::InvalidFadeOut => f.write_str("audio fade-out must be finite and nonnegative"),
            Self::FadesExceedDuration => {
                f.write_str("audio fades must fit without overlap in the clip")
            }
            Self::ResourceMismatch => f.write_str(
                "audio clip resource format or frame count differs from its validated source",
            ),
            Self::InvalidSceneTime => {
                f.write_str("audio query time must be finite and nonnegative")
            }
        }
    }
}

impl std::error::Error for AudioClipError {}

#[cfg(test)]
mod tests;
