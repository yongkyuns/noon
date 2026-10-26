//! Bounded PCM queries over compiled audio; no transport or runtime writes.

use noon_core::{AudioClipDefinition, AudioResource};

use crate::SceneInstance;

/// Maximum number of output sample frames in one borrowed mixer query.
///
/// Stereo mixing retains at most 256 KiB of temporary f64 samples. Callers can
/// choose smaller blocks and check cancellation between successive queries.
pub const MAX_AUDIO_MIX_BLOCK_FRAMES: usize = 16_384;

const MAX_EXACT_OUTPUT_FRAME: u64 = 1 << 53;

/// Validated interleaved PCM output format for a finite audio query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioMixFormat {
    sample_rate: u32,
    channels: u16,
}

impl AudioMixFormat {
    pub const STEREO_48_KHZ: Self = Self {
        sample_rate: 48_000,
        channels: 2,
    };

    /// Admit the same mono/stereo sample-rate range as immutable PCM resources.
    pub fn new(sample_rate: u32, channels: u16) -> Result<Self, AudioMixError> {
        if !(8_000..=192_000).contains(&sample_rate) {
            return Err(AudioMixError::UnsupportedSampleRate { sample_rate });
        }
        if !matches!(channels, 1 | 2) {
            return Err(AudioMixError::UnsupportedChannelCount { channels });
        }
        Ok(Self {
            sample_rate,
            channels,
        })
    }

    pub const fn sample_rate(self) -> u32 {
        self.sample_rate
    }

    pub const fn channels(self) -> u16 {
        self.channels
    }
}

/// Invalid requests and unrepresentable mixes leave the caller's output intact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioMixError {
    UnsupportedSampleRate {
        sample_rate: u32,
    },
    UnsupportedChannelCount {
        channels: u16,
    },
    InvalidSceneStart(f64),
    IncompleteOutputFrame {
        samples: usize,
        channels: u16,
    },
    BlockFrameLimitExceeded {
        frames: usize,
        limit: usize,
    },
    InvalidBlockFrameCapacity {
        frames: usize,
    },
    OutputFrameRangeOverflow,
    OutputFramePrecisionLoss {
        frame: u64,
    },
    NonFiniteSceneTime {
        frame: u64,
    },
    SceneTimePrecisionLoss {
        frame: u64,
    },
    ScratchAllocationFailed,
    ClipStorageAllocationFailed,
    NonFiniteMix {
        frame: u64,
        channel: u16,
    },
    OutputOutOfRange {
        frame: u64,
        channel: u16,
        value: f64,
    },
}

impl std::fmt::Display for AudioMixError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSampleRate { sample_rate } => write!(
                formatter,
                "audio mix sample rate must be between 8000 and 192000 Hz, received {sample_rate}"
            ),
            Self::UnsupportedChannelCount { channels } => write!(
                formatter,
                "audio mix requires mono or stereo output, received {channels} channels"
            ),
            Self::InvalidSceneStart(time) => write!(
                formatter,
                "audio mix requires a nonnegative finite scene start, received {time}"
            ),
            Self::IncompleteOutputFrame { samples, channels } => write!(
                formatter,
                "audio mix output has {samples} samples, not complete {channels}-channel frames"
            ),
            Self::BlockFrameLimitExceeded { frames, limit } => write!(
                formatter,
                "audio mix block has {frames} frames, exceeding its {limit}-frame limit"
            ),
            Self::InvalidBlockFrameCapacity { frames } => write!(
                formatter,
                "prepared audio mix capacity must be between 1 and {MAX_AUDIO_MIX_BLOCK_FRAMES} frames, received {frames}"
            ),
            Self::OutputFrameRangeOverflow => {
                formatter.write_str("audio mix output frame range overflows u64")
            }
            Self::OutputFramePrecisionLoss { frame } => write!(
                formatter,
                "audio mix frame index {frame} exceeds the exact f64 integer range"
            ),
            Self::NonFiniteSceneTime { frame } => write!(
                formatter,
                "audio mix frame {frame} maps to a nonfinite authored scene time"
            ),
            Self::SceneTimePrecisionLoss { frame } => write!(
                formatter,
                "audio mix scene time cannot distinguish frame {frame} from its successor"
            ),
            Self::ScratchAllocationFailed => {
                formatter.write_str("could not allocate bounded audio mix scratch storage")
            }
            Self::ClipStorageAllocationFailed => {
                formatter.write_str("could not retain immutable audio mix clip dependencies")
            }
            Self::NonFiniteMix { frame, channel } => write!(
                formatter,
                "audio mix is nonfinite at output frame {frame}, channel {channel}"
            ),
            Self::OutputOutOfRange {
                frame,
                channel,
                value,
            } => write!(
                formatter,
                "audio mix sample {value} at output frame {frame}, channel {channel} exceeds [-1, 1]; reduce authored gain"
            ),
        }
    }
}

impl std::error::Error for AudioMixError {}

struct OutputGrid {
    scene_start: f64,
    first_frame: u64,
    sample_rate: f64,
    frames: usize,
}

impl OutputGrid {
    fn validate(
        scene_start: f64,
        first_frame: u64,
        format: AudioMixFormat,
        samples: usize,
        max_block_frames: usize,
    ) -> Result<Self, AudioMixError> {
        if !scene_start.is_finite() || scene_start < 0.0 {
            return Err(AudioMixError::InvalidSceneStart(scene_start));
        }
        if !samples.is_multiple_of(usize::from(format.channels)) {
            return Err(AudioMixError::IncompleteOutputFrame {
                samples,
                channels: format.channels,
            });
        }
        let frames = samples / usize::from(format.channels);
        if frames > max_block_frames {
            return Err(AudioMixError::BlockFrameLimitExceeded {
                frames,
                limit: max_block_frames,
            });
        }
        let end = first_frame
            .checked_add(frames as u64)
            .ok_or(AudioMixError::OutputFrameRangeOverflow)?;
        if end > MAX_EXACT_OUTPUT_FRAME {
            return Err(AudioMixError::OutputFramePrecisionLoss { frame: end });
        }
        let grid = Self {
            scene_start,
            first_frame,
            sample_rate: f64::from(format.sample_rate),
            frames,
        };
        let mut previous = grid.time_at(0);
        if !previous.is_finite() {
            return Err(AudioMixError::NonFiniteSceneTime { frame: first_frame });
        }
        // Include the successor of the last requested sample. A one-frame block
        // must reject the same collapsed grid that a larger block would reject.
        for index in 1..=frames {
            let current = grid.time_at(index);
            if !current.is_finite() {
                return Err(AudioMixError::NonFiniteSceneTime {
                    frame: first_frame + index as u64,
                });
            }
            if current <= previous {
                return Err(AudioMixError::SceneTimePrecisionLoss {
                    frame: first_frame + index as u64 - 1,
                });
            }
            previous = current;
        }
        Ok(grid)
    }

    fn time_at(&self, index: usize) -> f64 {
        self.scene_start + (self.first_frame + index as u64) as f64 / self.sample_rate
    }

    /// Find the first output sample at or after a clip boundary. Searching the
    /// actual absolute sample grid avoids rounded ceil(time * rate) boundaries.
    fn lower_bound(&self, time: f64) -> usize {
        let mut low = 0;
        let mut high = self.frames;
        while low < high {
            let middle = low + (high - low) / 2;
            if self.time_at(middle) < time {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low
    }
}

#[derive(Debug)]
struct PreparedAudioClip {
    definition: AudioClipDefinition,
    pcm: AudioResource,
}

/// Immutable audio dependencies and reusable scratch for bounded PCM queries.
///
/// Preparation retains existing shared PCM allocations and copies only validated
/// clip definitions. This value has no runtime, playhead, publication or device;
/// every query supplies its authored-time origin and absolute sample index.
/// A host must retire it when its admitted immutable execution is replaced.
///
/// Mixing performs no allocation, locking, decoding or scene evaluation. Work
/// visits the retained audio clips and their intersecting output samples, using
/// at most the block capacity chosen at preparation. Destruction releases its
/// retained resources and scratch, so hosts must also keep destruction outside
/// their real-time callback.
#[derive(Debug)]
pub struct PreparedAudioMix {
    clips: Vec<PreparedAudioClip>,
    format: AudioMixFormat,
    scratch: Vec<f64>,
}

impl PreparedAudioMix {
    pub const fn format(&self) -> AudioMixFormat {
        self.format
    }

    /// Maximum output frames accepted without growing the prepared scratch.
    pub fn max_block_frames(&self) -> usize {
        self.scratch.len() / usize::from(self.format.channels)
    }

    /// Number of immutable clips visited by one block query.
    pub fn clip_count(&self) -> usize {
        self.clips.len()
    }

    /// Mix on the same explicit sample grid as [`SceneInstance::mix_audio_block`].
    ///
    /// Calls may arrive in any time order, including after a seek. Smaller blocks
    /// reuse the prepared scratch prefix; no previous samples or time cursor are
    /// retained. Invalid or out-of-range mixes leave all output samples intact.
    pub fn mix_audio_block(
        &mut self,
        scene_start: f64,
        first_output_frame: u64,
        output: &mut [f32],
    ) -> Result<(), AudioMixError> {
        let grid = OutputGrid::validate(
            scene_start,
            first_output_frame,
            self.format,
            output.len(),
            self.max_block_frames(),
        )?;
        mix_audio_clips(
            self.clips.iter().map(|clip| (&clip.definition, &clip.pcm)),
            &grid,
            self.format,
            &mut self.scratch[..output.len()],
            output,
        )
    }
}

impl SceneInstance {
    /// Retain this plan's immutable audio closure with reusable bounded scratch.
    ///
    /// This preparation may allocate; later [`PreparedAudioMix::mix_audio_block`]
    /// calls do not. PCM remains shared with the compiled plan and stays valid
    /// after the runtime is dropped. Preparation grants no playback admission:
    /// the host still owns finite-profile checks, its clock and device lifecycle.
    pub fn prepare_audio_mix(
        &self,
        format: AudioMixFormat,
        max_block_frames: usize,
    ) -> Result<PreparedAudioMix, AudioMixError> {
        if !(1..=MAX_AUDIO_MIX_BLOCK_FRAMES).contains(&max_block_frames) {
            return Err(AudioMixError::InvalidBlockFrameCapacity {
                frames: max_block_frames,
            });
        }
        let scratch = allocate_scratch(max_block_frames * usize::from(format.channels))?;
        let mut clips = Vec::new();
        clips
            .try_reserve_exact(self.compiled.audio_clips().len())
            .map_err(|_| AudioMixError::ClipStorageAllocationFailed)?;
        for clip in self.compiled.audio_clips() {
            let definition = *clip.definition();
            let pcm = self
                .compiled
                .resources()
                .audio(definition.resource())
                .expect("compiled audio owns its immutable PCM closure")
                .clone();
            clips.push(PreparedAudioClip { definition, pcm });
        }
        Ok(PreparedAudioMix {
            clips,
            format,
            scratch,
        })
    }

    /// Mix a bounded block of interleaved PCM without advancing Runtime.
    ///
    /// Sample `i` is evaluated at `scene_start + (first_output_frame + i) / rate`.
    /// The origin is an authored scene time; the integer frame index is relative
    /// to that fixed origin, not to `frame().time` or the previous block. Keeping
    /// both arguments on the same sample grid makes output block-size invariant.
    ///
    /// Shared clip evaluation owns onset, trim, looping and fades. Resampling
    /// linearly interpolates source frames, holding the last trimmed frame of a
    /// one-shot clip and wrapping its neighbor to the trim start for loops. Mono
    /// duplicates into stereo; stereo retains its channels, or averages to mono.
    ///
    /// Contributions accumulate in bounded f64 scratch. A final sample outside
    /// [-1, 1], or any nonfinite intermediate mix, rejects the whole block before
    /// writing output; no implicit normalization or clipping changes authored gain.
    /// Work visits each clip once per block, then only its intersecting samples.
    pub fn mix_audio_block(
        &self,
        scene_start: f64,
        first_output_frame: u64,
        format: AudioMixFormat,
        output: &mut [f32],
    ) -> Result<(), AudioMixError> {
        let grid = OutputGrid::validate(
            scene_start,
            first_output_frame,
            format,
            output.len(),
            MAX_AUDIO_MIX_BLOCK_FRAMES,
        )?;
        if output.is_empty() {
            return Ok(());
        }
        let mut scratch = allocate_scratch(output.len())?;
        let clips = self.compiled.audio_clips().iter().map(|clip| {
            let definition = clip.definition();
            let pcm = self
                .compiled
                .resources()
                .audio(definition.resource())
                .expect("compiled audio owns its immutable PCM closure");
            (definition, pcm)
        });
        mix_audio_clips(clips, &grid, format, &mut scratch, output)
    }
}

fn allocate_scratch(samples: usize) -> Result<Vec<f64>, AudioMixError> {
    let mut scratch = Vec::new();
    scratch
        .try_reserve_exact(samples)
        .map_err(|_| AudioMixError::ScratchAllocationFailed)?;
    scratch.resize(samples, 0.0);
    Ok(scratch)
}

fn mix_audio_clips<'a>(
    clips: impl Iterator<Item = (&'a AudioClipDefinition, &'a AudioResource)>,
    grid: &OutputGrid,
    format: AudioMixFormat,
    mixed: &mut [f64],
    output: &mut [f32],
) -> Result<(), AudioMixError> {
    if output.is_empty() {
        return Ok(());
    }
    mixed.fill(0.0);
    let output_channels = usize::from(format.channels);
    for (definition, pcm) in clips {
        let start = grid.lower_bound(definition.start_time());
        let end = grid.lower_bound(definition.end_time());
        if start == end {
            continue;
        }
        for index in start..end {
            let position = definition
                .state_at(grid.time_at(index))
                .expect("audio mix sample grid was validated")
                .expect("clip intersection includes only its half-open active interval");
            let lower = position.source_frame.floor() as usize;
            let upper = if lower + 1 < definition.source_end_frame() {
                lower + 1
            } else if definition.is_looped() {
                definition.source_start_frame()
            } else {
                lower
            };
            let fraction = position.source_frame - lower as f64;
            let left = interpolate(pcm, lower, upper, 0, fraction);
            let right = if pcm.channels() == 1 {
                left
            } else {
                interpolate(pcm, lower, upper, 1, fraction)
            };
            let samples = if output_channels == 1 {
                [(left + right) * 0.5, 0.0]
            } else {
                [left, right]
            };
            for (channel, sample) in samples[..output_channels].iter().enumerate() {
                let slot = &mut mixed[index * output_channels + channel];
                *slot += sample * position.gain;
                if !slot.is_finite() {
                    return Err(AudioMixError::NonFiniteMix {
                        frame: grid.first_frame + index as u64,
                        channel: channel as u16,
                    });
                }
            }
        }
    }
    for (index, &value) in mixed.iter().enumerate() {
        if !(-1.0..=1.0).contains(&value) {
            return Err(AudioMixError::OutputOutOfRange {
                frame: grid.first_frame + (index / output_channels) as u64,
                channel: (index % output_channels) as u16,
                value,
            });
        }
    }
    for (target, &value) in output.iter_mut().zip(mixed.iter()) {
        *target = value as f32;
    }
    Ok(())
}

fn interpolate(
    pcm: &AudioResource,
    lower: usize,
    upper: usize,
    channel: usize,
    fraction: f64,
) -> f64 {
    let channels = usize::from(pcm.channels());
    let lower = f64::from(pcm.samples()[lower * channels + channel]);
    let upper = f64::from(pcm.samples()[upper * channels + channel]);
    lower + (upper - lower) * fraction
}

#[cfg(test)]
mod tests;
