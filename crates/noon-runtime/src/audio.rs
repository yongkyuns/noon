//! Device instructions borrowed from the current coherent runtime time.
//!
//! There is no audio playhead or device in this module. A host obtains a fresh
//! view after an admitted play/seek/reanchor, then realizes the instructions on
//! its output device. Preparation and ordinary runtime evaluation are silent.

use noon_core::{AudioResource, AudioResourceHandle, SemanticNodeId};

use crate::SceneInstance;

/// A gain value at an authored offset relative to this instruction's start.
/// The first point is an immediate value; subsequent points are linear ramps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AudioGainPoint {
    pub offset: f64,
    pub gain: f64,
}

/// Bounded clip-wide envelope; looping the PCM never restarts these fades.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioGainEnvelope {
    points: [AudioGainPoint; 4],
    len: usize,
}

impl AudioGainEnvelope {
    pub fn points(&self) -> &[AudioGainPoint] {
        &self.points[..self.len]
    }

    fn push(&mut self, offset: f64, gain: f64) {
        if self.len > 0 && self.points[self.len - 1].offset == offset {
            self.points[self.len - 1].gain = gain;
        } else {
            self.points[self.len] = AudioGainPoint { offset, gain };
            self.len += 1;
        }
    }
}

/// Half-open loop region in seconds of the original immutable PCM resource.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioLoopRegion {
    pub start: f64,
    pub end: f64,
}

/// One future or active clip at the existing Runtime's current authored time.
///
/// All times are authored seconds. A host maps them to device time using its
/// existing presentation rate; the device clock never changes these semantics.
#[derive(Clone, Copy, Debug)]
pub struct AudioPlaybackClip<'a> {
    pub clip: SemanticNodeId,
    pub resource: AudioResourceHandle,
    pub pcm: &'a AudioResource,
    pub start_delay: f64,
    pub duration: f64,
    pub source_offset: f64,
    pub loop_region: Option<AudioLoopRegion>,
    pub gain_points: AudioGainEnvelope,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioPlaybackError {
    InvalidTime(f64),
}

impl std::fmt::Display for AudioPlaybackError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTime(time) => write!(
                formatter,
                "audio output requires a nonnegative finite scene time, received {time}"
            ),
        }
    }
}

impl std::error::Error for AudioPlaybackError {}

impl SceneInstance {
    pub fn has_audio(&self) -> bool {
        self.compiled.has_audio()
    }

    pub fn audio_end_time(&self) -> Option<f64> {
        self.compiled.audio_end_time()
    }

    /// Derive device instructions at the SAME committed time as `frame()`.
    ///
    /// This allocation-free traversal is intended for transport reanchors,
    /// not ordinary frames. Active audio cadence uses the existing timeline
    /// scheduler's event index. It includes future clips so a device can start
    /// them without a frame-rate-dependent onset. Expired clips are omitted.
    /// Seeking and querying repeatedly have no output side effects.
    pub fn audio_playback(
        &self,
    ) -> Result<impl Iterator<Item = AudioPlaybackClip<'_>>, AudioPlaybackError> {
        let time = self.frame.time;
        if !time.is_finite() || time < 0.0 {
            return Err(AudioPlaybackError::InvalidTime(time));
        }
        Ok(self.compiled.audio_clips().iter().filter_map(move |clip| {
            let definition = clip.definition();
            let origin = time.max(definition.start_time());
            let position = definition
                .state_at(origin)
                .expect("current audio time was validated")?;
            let pcm = self
                .compiled
                .resources()
                .audio(definition.resource())
                .expect("compiled audio owns its immutable PCM closure");
            let elapsed = origin - definition.start_time();
            let duration = definition.end_time() - origin;
            let mut gain_points = AudioGainEnvelope {
                points: [AudioGainPoint::default(); 4],
                len: 0,
            };
            gain_points.push(0.0, position.gain);
            let fade_in_end = definition.fade_in() - elapsed;
            if fade_in_end > 0.0 && fade_in_end < duration {
                gain_points.push(fade_in_end, definition.gain());
            }
            let fade_out_start = definition.duration() - definition.fade_out() - elapsed;
            if definition.fade_out() > 0.0 && fade_out_start > 0.0 && fade_out_start < duration {
                gain_points.push(fade_out_start, definition.gain());
            }
            // The natural end has no active state. Its left envelope limit is
            // zero with an authored fade-out, otherwise the authored gain.
            gain_points.push(
                duration,
                if definition.fade_out() > 0.0 {
                    0.0
                } else {
                    definition.gain()
                },
            );
            let rate = f64::from(pcm.sample_rate());
            Some(AudioPlaybackClip {
                clip: clip.clip(),
                resource: definition.resource(),
                pcm,
                start_delay: origin - time,
                duration,
                source_offset: position.source_frame / rate,
                loop_region: definition.is_looped().then(|| AudioLoopRegion {
                    start: definition.source_start_frame() as f64 / rate,
                    end: definition.source_end_frame() as f64 / rate,
                }),
                gain_points,
            })
        }))
    }
}

#[cfg(test)]
mod tests;
