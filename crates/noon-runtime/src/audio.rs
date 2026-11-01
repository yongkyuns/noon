//! Audio queries borrowed from compiled PCM and the existing authored timeline.
//!
//! There is no audio playhead or device in this module. A host obtains a fresh
//! view after an admitted play/seek/reanchor, then realizes the instructions on
//! its output device. Finite export can instead query bounded sample blocks at
//! explicit authored times. Preparation and ordinary evaluation remain silent.

mod mix;
pub use mix::{AudioMixError, AudioMixFormat, PreparedAudioMix, MAX_AUDIO_MIX_BLOCK_FRAMES};

use noon_core::{
    AudioClipDefinition, AudioClipError, AudioResource, AudioResourceHandle, SemanticNodeId,
};

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
    pub definition: &'a AudioClipDefinition,
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

    /// Common semantic scope of a nonempty immutable audio plan, if it has one.
    /// Session bootstrap uses this O(clips) query to classify a fixed soundtrack;
    /// it is not called during frame evaluation or local authored publication.
    pub fn uniform_audio_scope(&self) -> Option<SemanticNodeId> {
        let clips = self.compiled.audio_clips();
        let scope = clips.first()?.scope();
        clips
            .iter()
            .all(|clip| clip.scope() == scope)
            .then_some(scope)
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
        self.audio_playback_at(self.frame.time)
    }

    /// Derive immutable device instructions at an explicit authored time without
    /// evaluating or publishing a frame. Real worker transports use this to
    /// prepare resources once; the accepted player clock still owns playback.
    pub fn audio_playback_at(
        &self,
        time: f64,
    ) -> Result<impl Iterator<Item = AudioPlaybackClip<'_>>, AudioPlaybackError> {
        if !time.is_finite() || time < 0.0 {
            return Err(AudioPlaybackError::InvalidTime(time));
        }
        Ok(self.compiled.audio_clips().iter().filter_map(move |clip| {
            let definition = clip.definition();
            let pcm = self
                .compiled
                .resources()
                .audio(definition.resource())
                .expect("compiled audio owns its immutable PCM closure");
            AudioPlaybackClip::at_time(clip.clip(), definition, pcm, time)
                .expect("compiled audio and the query time were validated")
        }))
    }
}

impl<'a> AudioPlaybackClip<'a> {
    /// The shared phase/envelope projection with an explicit output horizon.
    /// A disposable cross-context resource mirror uses the delivered horizon
    /// carried by its accepted host anchor; having future PCM is not permission
    /// to schedule it. The cutoff preserves the authored envelope and loop phase.
    #[cfg(feature = "prepared-prefix")]
    pub fn at_time_before(
        clip: SemanticNodeId,
        definition: &'a AudioClipDefinition,
        pcm: &'a AudioResource,
        time: f64,
        available_until: f64,
    ) -> Result<Option<Self>, AudioClipError> {
        if !available_until.is_finite() || available_until < 0.0 {
            return Err(AudioClipError::InvalidSceneTime);
        }
        Ok(Self::at_time(clip, definition, pcm, time)?
            .and_then(|clip| clip.limit_duration(available_until - time - clip.start_delay)))
    }

    /// Clip a derived instruction to the readable prefix, not its authored
    /// definition. Preserve source phase and the original gain at the cutoff:
    /// availability is not a new fade-out or a new loop boundary.
    #[cfg(feature = "prepared-prefix")]
    pub(crate) fn limit_duration(mut self, maximum: f64) -> Option<Self> {
        debug_assert!(
            maximum.is_finite(),
            "prefix admission provides a finite horizon"
        );
        if maximum <= 0.0 {
            return None;
        }
        if maximum >= self.duration {
            return Some(self);
        }
        let mut envelope = AudioGainEnvelope {
            points: [AudioGainPoint::default(); 4],
            len: 0,
        };
        for point in self.gain_points.points() {
            if point.offset < maximum {
                envelope.push(point.offset, point.gain);
                continue;
            }
            // The positive cutoff follows the initial zero-offset point.
            let previous = envelope.points[envelope.len - 1];
            let fraction = (maximum - previous.offset) / (point.offset - previous.offset);
            envelope.push(
                maximum,
                previous.gain + fraction * (point.gain - previous.gain),
            );
            break;
        }
        self.duration = maximum;
        self.gain_points = envelope;
        Some(self)
    }

    /// The shared stateless projection used by the Runtime and a validated audio
    /// resource mirror at a genuine worker boundary. This owns no playback clock.
    pub fn at_time(
        clip: SemanticNodeId,
        definition: &'a AudioClipDefinition,
        pcm: &'a AudioResource,
        time: f64,
    ) -> Result<Option<Self>, AudioClipError> {
        if !time.is_finite() || time < 0.0 {
            return Err(AudioClipError::InvalidSceneTime);
        }
        definition.validate_resource(pcm)?;
        let origin = time.max(definition.start_time());
        let Some(position) = definition.state_at(origin)? else {
            return Ok(None);
        };
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
        gain_points.push(
            duration,
            if definition.fade_out() > 0.0 {
                0.0
            } else {
                definition.gain()
            },
        );
        let rate = f64::from(pcm.sample_rate());
        Ok(Some(Self {
            clip,
            definition,
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
        }))
    }
}

#[cfg(test)]
mod tests;
