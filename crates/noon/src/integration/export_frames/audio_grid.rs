//! Exact PCM coordinates derived from an admitted finite output frame grid.

use super::ExportFrameSummary;
use crate::integration::AudioMixFormat;

/// A derived audio range for one exported movie, not a playback clock or mix.
///
/// The full video grid is padded to a whole PCM frame using integer arithmetic.
/// Mixing starts at the absolute cropped scene origin. Frames at or after the
/// exclusive authored export end are silence, including an off-grid final stop.
/// Native sinks and external capture producers use these same coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportAudioGrid {
    scene_start: f64,
    frames: u64,
    mixed_frames: u64,
    format: AudioMixFormat,
    byte_len: u64,
}

impl ExportAudioGrid {
    pub fn new(
        sampling: ExportFrameSummary,
        format: AudioMixFormat,
    ) -> Result<Self, ExportAudioGridError> {
        if sampling.frames == 0
            || !sampling.start_time.is_finite()
            || sampling.start_time < 0.0
            || !sampling.end_time.is_finite()
            || sampling.end_time <= sampling.start_time
        {
            return Err(ExportAudioGridError::InvalidInterval);
        }
        let ticks = u128::from(sampling.frames)
            * u128::from(sampling.frame_rate.denominator())
            * u128::from(format.sample_rate());
        let frames = u64::try_from(ticks.div_ceil(u128::from(sampling.frame_rate.numerator())))
            .ok()
            .filter(|&frames| frames > 0 && frames <= (1_u64 << 53))
            .ok_or(ExportAudioGridError::UnrepresentableFrameCount)?;
        let byte_len = frames
            .checked_mul(u64::from(format.channels()) * 4)
            .ok_or(ExportAudioGridError::UnrepresentableFrameCount)?;
        let rate = f64::from(format.sample_rate());
        if !(sampling.start_time + frames as f64 / rate).is_finite() {
            return Err(ExportAudioGridError::InvalidInterval);
        }
        // Search the actual mixer grid instead of rounding (end - start) * rate.
        // Large origins and fractional boundaries must retain the same phase.
        let (mut low, mut high) = (0, frames);
        while low < high {
            let middle = low + (high - low) / 2;
            if sampling.start_time + middle as f64 / rate < sampling.end_time {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        Ok(Self {
            scene_start: sampling.start_time,
            frames,
            mixed_frames: low,
            format,
            byte_len,
        })
    }

    pub const fn scene_start(self) -> f64 {
        self.scene_start
    }
    pub const fn frames(self) -> u64 {
        self.frames
    }
    pub const fn mixed_frames(self) -> u64 {
        self.mixed_frames
    }
    pub const fn format(self) -> AudioMixFormat {
        self.format
    }
    pub const fn byte_len(self) -> u64 {
        self.byte_len
    }

    /// Number of leading frames to mix in an otherwise zero-filled block.
    /// The runtime mixer separately enforces its bounded-block and precision
    /// checks; this method only validates containment in the exported movie.
    pub fn mixed_frames_in_block(
        self,
        first: u64,
        frames: usize,
    ) -> Result<usize, ExportAudioGridError> {
        first
            .checked_add(frames as u64)
            .filter(|&end| end <= self.frames)
            .ok_or(ExportAudioGridError::BlockOutsideExport)?;
        Ok(self.mixed_frames.saturating_sub(first).min(frames as u64) as usize)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportAudioGridError {
    InvalidInterval,
    UnrepresentableFrameCount,
    BlockOutsideExport,
}

impl std::fmt::Display for ExportAudioGridError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidInterval => "export audio requires a nonempty finite nonnegative interval",
            Self::UnrepresentableFrameCount => "export audio sample count is not representable",
            Self::BlockOutsideExport => "audio block extends outside the admitted export grid",
        })
    }
}

impl std::error::Error for ExportAudioGridError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integration::{ExportFrameOptions, ExportFramePolicy, ExportStop, FrameRate};

    fn sampling(rate: FrameRate, start_frame: u64, end: f64) -> ExportFrameSummary {
        ExportFramePolicy::new(ExportFrameOptions {
            frame_rate: rate,
            start_frame,
            stop: ExportStop::EndTime(end),
            max_frames: 1000,
            max_transitions_per_sample: 64,
            final_hold_seconds: 0.0,
        })
        .unwrap()
        .finite_summary(end + 1.0)
        .unwrap()
    }

    #[test]
    fn fractional_movie_uses_integer_ceiling_and_absolute_crop_cutoff() {
        let sampling = sampling(FrameRate::new(30_000, 1_001).unwrap(), 15, 2.03);
        let grid = ExportAudioGrid::new(sampling, AudioMixFormat::STEREO_48_KHZ).unwrap();
        assert_eq!(grid.frames(), 73_674);
        assert_eq!(grid.scene_start(), 0.5005);
        assert_eq!(grid.byte_len(), 73_674 * 8);
        let expected_cutoff = (0..grid.frames())
            .find(|index| grid.scene_start() + *index as f64 / 48_000.0 >= sampling.end_time)
            .unwrap();
        assert_eq!(grid.mixed_frames(), expected_cutoff);
    }

    #[test]
    fn exact_cutoff_and_every_block_partition_preserve_silent_padding() {
        let sampling = sampling(FrameRate::new(16, 1).unwrap(), 2, 0.21875);
        let grid = ExportAudioGrid::new(sampling, AudioMixFormat::STEREO_48_KHZ).unwrap();
        assert_eq!(grid.frames(), 6_000);
        assert_eq!(grid.mixed_frames(), 4_500);
        for block in [1, 7, 1024, 4_500, 6_000] {
            let mut first = 0;
            let mut mixed = 0;
            while first < grid.frames() {
                let count = (grid.frames() - first).min(block) as usize;
                mixed += grid.mixed_frames_in_block(first, count).unwrap();
                first += count as u64;
            }
            assert_eq!(mixed, 4_500);
        }
        assert_eq!(grid.mixed_frames_in_block(6_000, 0), Ok(0));
        assert_eq!(
            grid.mixed_frames_in_block(6_000, 1),
            Err(ExportAudioGridError::BlockOutsideExport)
        );
        assert_eq!(
            grid.mixed_frames_in_block(u64::MAX, 1),
            Err(ExportAudioGridError::BlockOutsideExport)
        );
    }

    #[test]
    fn invalid_and_unrepresentable_ranges_fail_before_allocating_pcm() {
        let valid = sampling(FrameRate::new(30, 1).unwrap(), 0, 1.0);
        for sampling in [
            ExportFrameSummary { frames: 0, ..valid },
            ExportFrameSummary {
                start_time: -1.0,
                ..valid
            },
            ExportFrameSummary {
                end_time: f64::INFINITY,
                ..valid
            },
            ExportFrameSummary {
                end_time: 0.0,
                ..valid
            },
            ExportFrameSummary {
                frames: u64::MAX,
                ..valid
            },
        ] {
            assert!(ExportAudioGrid::new(sampling, AudioMixFormat::STEREO_48_KHZ).is_err());
        }
    }
}
