//! Native device I/O for the admitted finite immutable audio profile.
//!
//! The first admitted callback supplies a predicted DAC observation to the
//! existing native RealtimeClock. An integer output-frame offset keeps PCM
//! contiguous; it never advances Runtime or selects authored events. Preparation
//! retains shared PCM and scratch once. Callbacks only mix into that scratch,
//! convert samples, and publish bounded atomic observations.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamInstant};
use noon::integration::{AudioMixFormat, PreparedAudioMix, MAX_AUDIO_MIX_BLOCK_FRAMES};
use noon::ExecutionSession;

use crate::{NativeHostError, RealtimeClock};

const MIX_BLOCK_FRAMES: usize = 1_024;
const MAX_CLIPS: usize = 256;
/// This is the initial host profile's discontinuity policy, not a CPAL guarantee.
const MAX_CLOCK_ERROR: Duration = Duration::from_millis(50);
const MAX_OUTPUT_LATENCY: Duration = Duration::from_millis(500);
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(2);
const HEALTH_POLL: Duration = Duration::from_millis(20);
const ATTACK: Duration = Duration::from_millis(40);
const RELEASE: Duration = Duration::from_millis(60);
const VIEWER_RAMP: Duration = Duration::from_millis(20);

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AudioFault {
    Device = 1,
    Underrun = 2,
    Timestamp = 3,
    Discontinuity = 4,
    Buffer = 5,
    Mix = 6,
}

impl AudioFault {
    fn message(code: u8) -> &'static str {
        match code {
            2 => "audio device underrun; synchronized playback was stopped",
            3 => "audio device supplied an invalid or excessive playback latency",
            4 => {
                "audio device timing diverged by more than 50 ms; synchronized playback was stopped"
            }
            5 => "audio device supplied an unsupported callback buffer",
            6 => "authored audio mix is outside the supported finite PCM range",
            _ => "audio device was lost, rerouted, or invalidated; playback was stopped",
        }
    }
}

struct OutputState {
    // One stream lifetime; never reset or reused for a replacement stream.
    epoch: Instant,
    scene_origin: f64,
    admitted: AtomicBool,
    retired: AtomicBool,
    fault: AtomicU8,
    // Nanoseconds from epoch + 1; zero means no observation. The callback is
    // the only writer. Readers never wait for initialization or take a lock.
    anchor: AtomicU64,
    heartbeat: AtomicU64,
    // Host commands are bounded atomic targets. Only the callback advances
    // their sample-grid envelopes; they never change authored gain or time.
    viewer_gain: AtomicU64,
    release_requested: AtomicBool,
    release_end: AtomicU64,
}

impl OutputState {
    fn new(epoch: Instant, scene_origin: f64, viewer_gain: f64) -> Self {
        Self {
            epoch,
            scene_origin,
            admitted: AtomicBool::new(false),
            retired: AtomicBool::new(false),
            fault: AtomicU8::new(0),
            anchor: AtomicU64::new(0),
            heartbeat: AtomicU64::new(0),
            viewer_gain: AtomicU64::new(viewer_gain.to_bits()),
            release_requested: AtomicBool::new(false),
            release_end: AtomicU64::new(0),
        }
    }

    fn encode(&self, time: Instant) -> Option<u64> {
        u64::try_from(time.checked_duration_since(self.epoch)?.as_nanos())
            .ok()?
            .checked_add(1)
    }

    fn decode(&self, value: u64) -> Option<Instant> {
        self.epoch
            .checked_add(Duration::from_nanos(value.checked_sub(1)?))
    }

    fn clock(&self) -> Option<RealtimeClock> {
        self.decode(self.anchor.load(Ordering::Acquire))
            .map(|wall| RealtimeClock::new(wall, self.scene_origin))
    }

    fn drained(&self, audio_end: f64, now: Instant) -> bool {
        self.clock()
            .and_then(|clock| clock.wall_deadline(audio_end))
            .and_then(|end| end.checked_add(MAX_CLOCK_ERROR))
            .is_some_and(|end| now >= end)
    }

    fn release(&self) {
        // An unobserved start must never be resurrected by a late picture or
        // callback. The final callback retirement check also covers this race.
        self.release_requested.store(true, Ordering::Release);
        if self.anchor.load(Ordering::Acquire) == 0 {
            self.retired.store(true, Ordering::Release);
        }
    }

    fn released(&self, now: Instant) -> bool {
        self.retired.load(Ordering::Acquire)
            || self
                .decode(self.release_end.load(Ordering::Acquire))
                .and_then(|end| end.checked_add(MAX_CLOCK_ERROR))
                .is_some_and(|end| now >= end)
    }

    fn fail(&self, fault: AudioFault) {
        let _ = self
            .fault
            .compare_exchange(0, fault as u8, Ordering::Release, Ordering::Relaxed);
    }

    fn check(&self, now: Instant) -> Result<(), NativeHostError> {
        let fault = self.fault.load(Ordering::Acquire);
        if fault != 0 {
            return Err(NativeHostError::Audio(AudioFault::message(fault).into()));
        }
        let last_callback = self
            .decode(self.heartbeat.load(Ordering::Acquire))
            .unwrap_or(self.epoch);
        if now.saturating_duration_since(last_callback) > CALLBACK_TIMEOUT {
            return Err(NativeHostError::Audio(
                "audio device stopped delivering callbacks".into(),
            ));
        }
        Ok(())
    }
}

/// Linear output gain on the already selected device sample grid. This is an
/// envelope, not a playback clock: it cannot select or advance authored time.
#[derive(Clone, Copy)]
struct GainRamp {
    first_frame: u64,
    frames: u64,
    from: f64,
    to: f64,
}

impl GainRamp {
    fn constant(gain: f64) -> Self {
        Self {
            first_frame: 0,
            frames: 0,
            from: gain,
            to: gain,
        }
    }

    fn value_at(self, frame: u64) -> f64 {
        let elapsed = frame.saturating_sub(self.first_frame);
        if elapsed >= self.frames {
            self.to
        } else {
            self.from + (self.to - self.from) * elapsed as f64 / self.frames as f64
        }
    }

    fn retarget(&mut self, frame: u64, target: f64, frames: u64) {
        if target != self.to {
            *self = Self {
                first_frame: frame,
                frames,
                from: self.value_at(frame),
                to: target,
            };
        }
    }
}

fn ramp_frames(duration: Duration, sample_rate: u32) -> u64 {
    // These fixed millisecond durations and the admitted rates fit exactly in
    // integer arithmetic; round up so a ramp is never shorter than requested.
    (duration.as_millis() as u64 * u64::from(sample_rate)).div_ceil(1_000)
}

struct OutputCallback {
    state: Arc<OutputState>,
    mixer: PreparedAudioMix,
    samples: Vec<f32>,
    first_playback: Option<StreamInstant>,
    // Position in the output buffer grid, never the Runtime's authored playhead.
    next_frame: u64,
    viewer_ramp: GainRamp,
    transport_ramp: GainRamp,
    releasing: bool,
}

impl OutputCallback {
    fn new(state: Arc<OutputState>, mixer: PreparedAudioMix) -> Self {
        let samples = vec![0.0; mixer.max_block_frames() * usize::from(mixer.format().channels())];
        let viewer_gain = f64::from_bits(state.viewer_gain.load(Ordering::Acquire));
        let attack_frames = ramp_frames(ATTACK, mixer.format().sample_rate());
        Self {
            state,
            mixer,
            samples,
            first_playback: None,
            next_frame: 0,
            viewer_ramp: GainRamp::constant(viewer_gain),
            transport_ramp: GainRamp {
                first_frame: 0,
                frames: attack_frames,
                from: 0.0,
                to: 1.0,
            },
            releasing: false,
        }
    }

    fn render<T: SizedSample + FromSample<f32>>(
        &mut self,
        output: &mut [T],
        now: Instant,
        timestamp: cpal::OutputStreamTimestamp,
    ) {
        output.fill(T::EQUILIBRIUM);
        let Some(heartbeat) = self.state.encode(now) else {
            self.state.fail(AudioFault::Timestamp);
            return;
        };
        self.state.heartbeat.store(heartbeat, Ordering::Release);
        if self.state.retired.load(Ordering::Acquire)
            || self.state.fault.load(Ordering::Acquire) != 0
            || !self.state.admitted.load(Ordering::Acquire)
        {
            return;
        }
        let channels = usize::from(self.mixer.format().channels());
        let frames = output.len() / channels;
        if !output.len().is_multiple_of(channels) || frames > MAX_AUDIO_MIX_BLOCK_FRAMES {
            self.state.fail(AudioFault::Buffer);
            return;
        }
        if frames == 0 {
            return;
        }
        let Some(latency) = timestamp
            .playback
            .checked_duration_since(timestamp.callback)
            .filter(|latency| *latency <= MAX_OUTPUT_LATENCY)
        else {
            self.state.fail(AudioFault::Timestamp);
            return;
        };
        let Some(predicted_wall) = now.checked_add(latency) else {
            self.state.fail(AudioFault::Timestamp);
            return;
        };
        let viewer_gain = f64::from_bits(self.state.viewer_gain.load(Ordering::Acquire));
        if let Some(first) = self.first_playback {
            let elapsed = self.next_frame as f64 / f64::from(self.mixer.format().sample_rate());
            let expected = Duration::from_secs_f64(elapsed);
            let stream_elapsed = timestamp.playback.checked_duration_since(first);
            let wall_elapsed = self
                .state
                .clock()
                .and_then(|clock| predicted_wall.checked_duration_since(clock.wall_origin));
            if stream_elapsed.is_none_or(|actual| actual.abs_diff(expected) > MAX_CLOCK_ERROR)
                || wall_elapsed.is_none_or(|actual| actual.abs_diff(expected) > MAX_CLOCK_ERROR)
            {
                self.state.fail(AudioFault::Discontinuity);
                return;
            }
        } else {
            let Some(anchor) = self.state.encode(predicted_wall) else {
                self.state.fail(AudioFault::Timestamp);
                return;
            };
            self.first_playback = Some(timestamp.playback);
            // A viewer change made while preparation was silent must be in
            // effect before the first audible sample, especially an initial
            // mute. There is no previously audible gain to ramp from yet.
            self.viewer_ramp = GainRamp::constant(viewer_gain);
            self.state.anchor.store(anchor, Ordering::Release);
        }
        let sample_rate = self.mixer.format().sample_rate();
        self.viewer_ramp.retarget(
            self.next_frame,
            viewer_gain,
            ramp_frames(VIEWER_RAMP, sample_rate),
        );
        if self.state.release_requested.load(Ordering::Acquire) && !self.releasing {
            // Release is bounded by 60 ms, including on rates whose sample
            // grid cannot represent that duration exactly. Round down here.
            let frames = RELEASE.as_millis() as u64 * u64::from(sample_rate) / 1_000;
            self.transport_ramp.retarget(self.next_frame, 0.0, frames);
            self.releasing = true;
            let Some(end) = predicted_wall
                .checked_add(Duration::from_secs_f64(
                    frames as f64 / f64::from(sample_rate),
                ))
                .and_then(|end| self.state.encode(end))
            else {
                self.state.fail(AudioFault::Timestamp);
                return;
            };
            self.state.release_end.store(end, Ordering::Release);
        }
        for chunk in output.chunks_mut(self.samples.len()) {
            let samples = &mut self.samples[..chunk.len()];
            if self
                .mixer
                .mix_audio_block(self.state.scene_origin, self.next_frame, samples)
                .is_err()
            {
                // The device receives no partial block when a later chunk rejects.
                output.fill(T::EQUILIBRIUM);
                self.state.fail(AudioFault::Mix);
                return;
            }
            for (offset, (targets, samples)) in chunk
                .chunks_exact_mut(channels)
                .zip(samples.chunks_exact(channels))
                .enumerate()
            {
                let frame = self.next_frame + offset as u64;
                let gain = self.transport_ramp.value_at(frame) * self.viewer_ramp.value_at(frame);
                for (target, &sample) in targets.iter_mut().zip(samples) {
                    let sample = (f64::from(sample) * gain) as f32;
                    // Integer PCM has an asymmetric positive endpoint. CPAL/dasp's
                    // conversion from +1.0 can wrap; saturate only the representation.
                    let sample = if T::FORMAT.is_int() || T::FORMAT.is_uint() {
                        sample.min(f32::from_bits(1.0_f32.to_bits() - 1))
                    } else {
                        sample
                    };
                    *target = T::from_sample(sample);
                }
            }
            self.next_frame += (chunk.len() / channels) as u64;
        }
        // A concurrent retirement/error never leaves a newly filled stale block.
        if self.state.retired.load(Ordering::Acquire)
            || self.state.fault.load(Ordering::Acquire) != 0
        {
            output.fill(T::EQUILIBRIUM);
        }
    }
}

pub(super) struct NativeAudioOutput {
    stream: cpal::Stream,
    state: Arc<OutputState>,
    audio_end: f64,
}

impl NativeAudioOutput {
    pub(super) fn prepare(
        session: &ExecutionSession,
        viewer_gain: f64,
    ) -> Result<Self, NativeHostError> {
        validate_gain(viewer_gain)?;
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| {
            NativeHostError::Audio("no default output device is available".into())
        })?;
        let supported = choose_config(&device)?;
        let format = AudioMixFormat::new(supported.sample_rate(), supported.channels())
            .map_err(|error| NativeHostError::Audio(error.to_string()))?;
        let mixer = session
            .prepare_audio_mix(format, MIX_BLOCK_FRAMES)
            .map_err(|error| NativeHostError::Audio(error.to_string()))?;
        let state = Arc::new(OutputState::new(
            Instant::now(),
            session.frame().time,
            viewer_gain,
        ));
        let callback = OutputCallback::new(state.clone(), mixer);
        let mut config = supported.config();
        config.buffer_size = device_buffer_size(format.sample_rate(), *supported.buffer_size());
        #[cfg(test)]
        let requested_buffer = config.buffer_size;
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, config, callback),
            SampleFormat::F64 => build::<f64>(&device, config, callback),
            SampleFormat::I8 => build::<i8>(&device, config, callback),
            SampleFormat::I16 => build::<i16>(&device, config, callback),
            SampleFormat::I24 => build::<cpal::I24>(&device, config, callback),
            SampleFormat::I32 => build::<i32>(&device, config, callback),
            SampleFormat::I64 => build::<i64>(&device, config, callback),
            SampleFormat::U8 => build::<u8>(&device, config, callback),
            SampleFormat::U16 => build::<u16>(&device, config, callback),
            SampleFormat::U32 => build::<u32>(&device, config, callback),
            SampleFormat::U64 => build::<u64>(&device, config, callback),
            _ => {
                return Err(NativeHostError::Audio(
                    "unsupported device PCM sample format".into(),
                ))
            }
        }
        .map_err(|error| NativeHostError::Audio(error.to_string()))?;
        #[cfg(test)]
        eprintln!(
            "native audio fixture: sample_rate={} requested_buffer_frames={requested_buffer:?} negotiated_buffer_frames={:?}",
            format.sample_rate(),
            stream.buffer_size(),
        );
        stream
            .play()
            .map_err(|error| NativeHostError::Audio(error.to_string()))?;
        Ok(Self {
            stream,
            state,
            audio_end: session.audio_end_time().unwrap_or(0.0),
        })
    }

    pub(super) fn admit(&self) {
        if !self.state.release_requested.load(Ordering::Acquire) {
            self.state.admitted.store(true, Ordering::Release);
        }
    }

    pub(super) fn clock(&self) -> Option<RealtimeClock> {
        self.state.clock()
    }

    pub(super) fn drained(&self, now: Instant) -> bool {
        self.state.drained(self.audio_end, now)
    }

    pub(super) fn set_gain(&self, gain: f64) -> Result<(), NativeHostError> {
        validate_gain(gain)?;
        self.state
            .viewer_gain
            .store(gain.to_bits(), Ordering::Release);
        Ok(())
    }

    pub(super) fn release(&self) {
        self.state.release();
    }

    pub(super) fn released(&self, now: Instant) -> bool {
        self.state.released(now) || self.drained(now)
    }

    pub(super) fn check(&self, now: Instant) -> Result<(), NativeHostError> {
        self.state.check(now)
    }
}

fn device_buffer_size(sample_rate: u32, supported: cpal::SupportedBufferSize) -> cpal::BufferSize {
    match supported {
        cpal::SupportedBufferSize::Range { min, max } => {
            // This is media playback, so request consistent 20 ms scheduling
            // headroom across admitted rates, rather than a fixed frame count
            // that becomes an accidental low-latency request at high rates.
            // Devices may negotiate a different period; callback size and
            // observed latency/discontinuity guards remain authoritative.
            cpal::BufferSize::Fixed(sample_rate.div_ceil(50).clamp(min, max))
        }
        cpal::SupportedBufferSize::Unknown => cpal::BufferSize::Default,
    }
}

fn validate_gain(gain: f64) -> Result<(), NativeHostError> {
    if !gain.is_finite() || !(0.0..=1.0).contains(&gain) {
        Err(NativeHostError::Audio(
            "viewer volume must be finite and between zero and one".into(),
        ))
    } else {
        Ok(())
    }
}

impl Drop for NativeAudioOutput {
    fn drop(&mut self) {
        self.state.retired.store(true, Ordering::Release);
        // The stream is destroyed on the host thread. pause is only best-effort;
        // it is not portable flushing, and replacement always creates a new stream.
        let _ = self.stream.pause();
    }
}

fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut callback: OutputCallback,
) -> Result<cpal::Stream, cpal::Error> {
    let state = callback.state.clone();
    device.build_output_stream(
        config,
        move |samples: &mut [T], info| callback.render(samples, Instant::now(), info.timestamp()),
        move |error| {
            // Error callbacks can also be real-time callbacks. No formatting,
            // logging, channel send, device teardown, or event-loop call here.
            match error.kind() {
                cpal::ErrorKind::RealtimeDenied => {}
                cpal::ErrorKind::Xrun => state.fail(AudioFault::Underrun),
                _ => state.fail(AudioFault::Device),
            }
        },
        None,
    )
}

fn supported_pcm(format: SampleFormat) -> bool {
    matches!(
        format,
        SampleFormat::F32
            | SampleFormat::F64
            | SampleFormat::I8
            | SampleFormat::I16
            | SampleFormat::I24
            | SampleFormat::I32
            | SampleFormat::I64
            | SampleFormat::U8
            | SampleFormat::U16
            | SampleFormat::U32
            | SampleFormat::U64
    )
}

fn choose_config(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig, NativeHostError> {
    let configs = device
        .supported_output_configs()
        .map_err(|error| NativeHostError::Audio(error.to_string()))?;
    configs
        .filter(|range| matches!(range.channels(), 1 | 2) && supported_pcm(range.sample_format()))
        .filter_map(|range| {
            let min = range.min_sample_rate().max(8_000);
            let max = range.max_sample_rate().min(192_000);
            (min <= max).then(|| range.with_sample_rate(48_000_u32.clamp(min, max)))
        })
        .max_by_key(|config| {
            (
                config.channels(),
                config.sample_format() == SampleFormat::F32,
                config.sample_rate() == 48_000,
            )
        })
        .ok_or_else(|| {
            NativeHostError::Audio(
                "device has no supported mono/stereo PCM configuration at 8–192 kHz".into(),
            )
        })
}

pub(super) fn admit_profile(
    source: &dyn crate::execution_source::NativeExecutionSource,
) -> Result<(), NativeHostError> {
    let session = source.session();
    if source.source_active() || session.finite_timeline_end_time().is_none() {
        return Err(NativeHostError::Audio("native audio requires a complete finite immutable scene without source continuations or required callbacks (#2263)".into()));
    }
    if session
        .audio_playback_at(0.0)
        .map_err(|error| NativeHostError::Audio(error.to_string()))?
        .take(MAX_CLIPS + 1)
        .count()
        > MAX_CLIPS
    {
        return Err(NativeHostError::Audio(
            "native audio supports at most 256 immutable clips in one playback (#2263)".into(),
        ));
    }
    Ok(())
}

mod host;

#[cfg(test)]
mod tests;
