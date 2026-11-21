//! Scene-time sampling shared by realtime video, seeks, and offline export.

use std::{fmt::Debug, io, ops::Range, sync::Arc, time::Duration};

use crate::{AudioClipOptions, AudioResource};

/// An immutable source of scene raster samples. Opening a decode window is a
/// host operation performed outside the semantic store and execution plan.
pub trait VideoFrameSource: Debug + Send + Sync {
    fn dimensions(&self) -> (u32, u32);
    fn frame_index(&self) -> &VideoFrameIndex;
    /// Identifier used by a host that owns encoded media in another context.
    fn external_asset_id(&self) -> Option<u32> {
        None
    }
    fn open_window(
        self: Arc<Self>,
        wake: Box<dyn Fn() + Send>,
    ) -> io::Result<Box<dyn VideoFrameWindow>>;
}

/// A disposable host decode window. The existing scene clock selects which
/// index to request. Only offline output waits synchronously for pixels.
pub trait VideoFrameWindow {
    fn try_frame(&self, index: usize) -> io::Result<Option<Arc<Vec<u8>>>>;
    fn wait_frame(&self, index: usize, timeout: Duration) -> io::Result<Arc<Vec<u8>>>;
}

#[derive(Clone, Debug)]
pub struct VideoRasterResource {
    source: Arc<dyn VideoFrameSource>,
    timing: VideoClipTiming,
}

impl VideoRasterResource {
    pub fn new(
        source: Arc<dyn VideoFrameSource>,
        scene_start: f64,
        trim: Range<f64>,
    ) -> Result<Self, VideoTimingError> {
        let timing = VideoClipTiming::new(scene_start, trim, source.frame_index())?;
        Ok(Self { source, timing })
    }

    pub fn source(&self) -> &Arc<dyn VideoFrameSource> {
        &self.source
    }

    pub const fn timing(&self) -> VideoClipTiming {
        self.timing
    }
}

impl PartialEq for VideoRasterResource {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.source, &other.source) && self.timing == other.timing
    }
}

impl Eq for VideoRasterResource {}

/// Decoded-frame presentation times in ascending order, relative to the first
/// video frame. These are source timestamps in seconds, including for a
/// variable-frame-rate source.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoFrameIndex {
    timestamps: Box<[f64]>,
    duration: f64,
}

impl VideoFrameIndex {
    pub fn new(timestamps: impl Into<Box<[f64]>>, duration: f64) -> Result<Self, VideoTimingError> {
        let timestamps = timestamps.into();
        if timestamps.first() != Some(&0.0)
            || !duration.is_finite()
            || timestamps
                .iter()
                .zip(timestamps.iter().skip(1).chain([&duration]))
                .any(|(&current, &next)| !current.is_finite() || current >= next)
        {
            return Err(VideoTimingError::FrameIndex);
        }
        Ok(Self {
            timestamps,
            duration,
        })
    }

    pub fn timestamps(&self) -> &[f64] {
        &self.timestamps
    }

    pub const fn duration(&self) -> f64 {
        self.duration
    }
}

/// The trim interval is half-open. The clip advances at authored scene speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoClipTiming {
    scene_start: f64,
    scene_end: f64,
    source_start: f64,
    source_end: f64,
}

impl VideoClipTiming {
    pub fn new(
        scene_start: f64,
        source: Range<f64>,
        index: &VideoFrameIndex,
    ) -> Result<Self, VideoTimingError> {
        let scene_end = scene_start + (source.end - source.start);
        if !scene_start.is_finite()
            || scene_start < 0.0
            || !scene_end.is_finite()
            || scene_end <= scene_start
            || !source.start.is_finite()
            || source.start < 0.0
            || source.end > index.duration
        {
            return Err(VideoTimingError::ClipRange);
        }
        Ok(Self {
            scene_start,
            scene_end,
            source_start: source.start,
            source_end: source.end,
        })
    }

    pub const fn scene_start(self) -> f64 {
        self.scene_start
    }

    pub const fn scene_end(self) -> f64 {
        self.scene_end
    }

    pub fn source_range(self) -> Range<f64> {
        self.source_start..self.source_end
    }

    /// Select a frame by PTS, independently of any earlier sample. Compare
    /// boundaries in scene coordinates so seeking to a published transition and
    /// waking at that transition both select the new frame.
    pub fn frame_at(self, index: &VideoFrameIndex, scene_time: f64) -> Option<usize> {
        if !(self.scene_start..self.scene_end).contains(&scene_time) {
            return None;
        }
        Some(
            index
                .timestamps
                .partition_point(|&pts| self.scene_time(pts) <= scene_time)
                .saturating_sub(1),
        )
    }

    /// The next scene-time wake, including clip entry and exit. At a timestamp
    /// shared by the renderer and the clock, the boundary is already consumed.
    pub fn next_boundary(self, index: &VideoFrameIndex, scene_time: f64) -> Option<f64> {
        if !scene_time.is_finite() {
            return None;
        }
        if scene_time < self.scene_start {
            return Some(self.scene_start);
        }
        let frame = self.frame_at(index, scene_time)?;
        let next = index
            .timestamps
            .get(frame + 1)
            .map_or(self.scene_end, |&pts| self.scene_time(pts));
        Some(next.min(self.scene_end))
    }

    /// Schedule the portion of the decoded PCM that overlaps the video trim.
    /// The offset is the timestamp of PCM frame zero relative to video frame
    /// zero, after removing any encoder priming. Keep the fractional offset in
    /// scene time even though the mixer takes an integer source frame range.
    pub fn audio_options(
        self,
        pcm: &AudioResource,
        audio_offset: f64,
    ) -> Result<Option<AudioClipOptions>, VideoTimingError> {
        let rate = f64::from(pcm.sample_rate());
        let frame_limit = pcm.frames() as f64;
        if !audio_offset.is_finite() {
            return Err(VideoTimingError::AudioOffset);
        }
        let first_frame = ((self.source_start - audio_offset) * rate)
            .ceil()
            .clamp(0.0, frame_limit) as usize;
        let past_last_frame = ((self.source_end - audio_offset) * rate)
            .ceil()
            .clamp(0.0, frame_limit) as usize;
        if first_frame >= past_last_frame {
            return Ok(None);
        }
        let start_time = self
            .scene_time(audio_offset + first_frame as f64 / rate)
            .max(self.scene_start);
        let duration =
            (self.scene_end - start_time).min((past_last_frame - first_frame) as f64 / rate);
        if duration <= 0.0 {
            return Ok(None);
        }
        Ok(Some(AudioClipOptions {
            start_time,
            source_start_frame: first_frame,
            source_end_frame: Some(past_last_frame),
            duration: Some(duration),
            ..AudioClipOptions::default()
        }))
    }

    fn scene_time(self, source_time: f64) -> f64 {
        self.scene_start + (source_time - self.source_start)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoTimingError {
    FrameIndex,
    ClipRange,
    AudioOffset,
}

impl std::fmt::Display for VideoTimingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::FrameIndex => {
                "video requires strictly ordered presentation timestamps and a finite duration"
            }
            Self::ClipRange => {
                "video trim must be a positive interval within the asset at a finite scene start"
            }
            Self::AudioOffset => "video soundtrack offset must be finite",
        })
    }
}

impl std::error::Error for VideoTimingError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioClipDefinition, AudioResourceArena};

    #[test]
    fn trimmed_vfr_clip_samples_identically_in_either_direction() {
        let index = VideoFrameIndex::new(vec![0.0, 0.1, 0.37, 0.6], 0.8).unwrap();
        let clip = VideoClipTiming::new(1.2, 0.15..0.7, &index).unwrap();
        let second = clip.next_boundary(&index, 1.2).unwrap();
        let third = clip.next_boundary(&index, second).unwrap();
        let times = [1.1, clip.scene_start(), second, third, clip.scene_end()];
        let frames = [None, Some(1), Some(2), Some(3), None];
        for (time, expected) in times
            .into_iter()
            .zip(frames)
            .chain(times.into_iter().zip(frames).rev())
        {
            assert_eq!(clip.frame_at(&index, time), expected);
        }
        assert_eq!(clip.next_boundary(&index, 0.0), Some(1.2));
        assert_eq!(clip.next_boundary(&index, third), Some(clip.scene_end()));
        assert_eq!(clip.next_boundary(&index, clip.scene_end()), None);
        assert!(VideoFrameIndex::new(vec![0.0, 0.37, 0.37], 0.8).is_err());
        assert!(VideoClipTiming::new(1.2, 0.15..0.9, &index).is_err());
    }

    #[test]
    fn trimmed_soundtrack_uses_the_video_origin_and_ends_at_trim() {
        const RATE: u32 = 48_000;
        let index = VideoFrameIndex::new(vec![0.0, 0.25, 0.5], 0.75).unwrap();
        let clip = VideoClipTiming::new(1.25, 0.25..0.625, &index).unwrap();
        let mut audio = AudioResourceArena::default();
        let pcm = AudioResource::from_interleaved(RATE, 1, vec![0.0; 48_000], 256_000).unwrap();
        let handle = audio.insert(pcm.clone()).unwrap();
        let offset = 0.125;
        let options = clip.audio_options(&pcm, offset).unwrap().unwrap();
        let schedule = AudioClipDefinition::new(handle, &pcm, options).unwrap();

        assert_eq!(options.source_start_frame, 6_000);
        assert_eq!(options.source_end_frame, Some(24_000));
        assert_eq!(options.start_time, 1.25);
        assert!(schedule.state_at(1.249).unwrap().is_none());
        assert_eq!(clip.frame_at(&index, 1.5), Some(2));
        assert_eq!(
            schedule.state_at(1.5).unwrap().unwrap().source_frame,
            18_000.0
        );
        assert!(schedule.state_at(clip.scene_end()).unwrap().is_none());
        assert!(clip.audio_options(&pcm, 0.75).unwrap().is_none());
        let delayed = clip.audio_options(&pcm, 0.375).unwrap().unwrap();
        assert_eq!(delayed.start_time, 1.375);
        assert_eq!(delayed.source_start_frame, 0);
    }
}
