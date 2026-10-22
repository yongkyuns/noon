//! A Small Discovery — an original, gently curious instrumental bed.
//!
//! This is an offline asset generator, not an audio runtime. It uses a fixed
//! original score, additive oscillators, and three short synthetic reflections.
//! No recordings, samples, external melodies, models, or services are used.
//! Repeated calls with the same rate on the same target produce identical PCM.
//! Floating-point transcendental functions may differ slightly across targets.
//!
//! Software implementation: MIT, as declared by the Noon crate. The original
//! composition represented by CHORDS and PHRASES, and audio rendered from it,
//! are dedicated under CC0-1.0; see the scoped notice in
//! `crates/noon/examples/render_background_music/README.md`.

use std::f64::consts::{FRAC_PI_2, TAU};
use std::fmt;

pub const TITLE: &str = "A Small Discovery";
pub const DURATION_SECONDS: u32 = 80;
pub const CHANNELS: usize = 2;

/// Interleaved stereo PCM, with no public mutation after rendering.
#[derive(Debug)]
pub struct MusicPcm {
    sample_rate: u32,
    samples: Box<[f32]>,
}

impl MusicPcm {
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
    /// Consume prepared PCM so scene resource admission does not retain a second buffer.
    pub fn into_samples(self) -> Box<[f32]> {
        self.samples
    }
    pub fn frames(&self) -> usize {
        self.samples.len() / CHANNELS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicError {
    UnsupportedSampleRate,
    AllocationFailed,
}

impl fmt::Display for MusicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnsupportedSampleRate => "music sample rate must be 8000..=96000 Hz",
            Self::AllocationFailed => "could not allocate music PCM",
        })
    }
}

impl std::error::Error for MusicError {}

// Sixteen bars at 48 BPM. Each harmony lasts two bars / ten seconds.
// Voicings: Dadd9, Bm7, Gmaj9, Asus2, Dadd9/F#, G6, Em9, Asus2.
const CHORDS: [[u8; 4]; 8] = [
    [50, 57, 64, 66],
    [47, 54, 57, 62],
    [43, 50, 57, 66],
    [45, 52, 59, 64],
    [42, 50, 57, 64],
    [43, 50, 59, 64],
    [40, 47, 55, 66],
    [45, 52, 59, 64],
];

// (Beat within the two-bar phrase, MIDI note, quiet dynamic variation).
// The score is explicit so musical edits stay reviewable and reproducible.
const PHRASES: [&[(f64, u8, f64)]; 8] = [
    &[
        (0.50, 66, 0.78),
        (3.25, 76, 0.48),
        (5.25, 69, 0.60),
        (7.25, 64, 0.54),
    ],
    &[
        (1.00, 66, 0.63),
        (3.00, 74, 0.48),
        (5.50, 73, 0.46),
        (7.25, 69, 0.57),
    ],
    &[
        (0.75, 71, 0.58),
        (2.50, 69, 0.50),
        (5.00, 66, 0.60),
        (7.25, 74, 0.44),
    ],
    &[(1.50, 76, 0.43), (4.00, 71, 0.54), (6.75, 69, 0.48)],
    &[
        (0.50, 64, 0.65),
        (2.25, 66, 0.57),
        (4.50, 69, 0.52),
        (7.00, 76, 0.42),
    ],
    &[(1.25, 71, 0.53), (3.50, 76, 0.42), (6.50, 69, 0.55)],
    &[
        (0.75, 67, 0.60),
        (3.25, 66, 0.48),
        (5.50, 71, 0.50),
        (7.25, 76, 0.41),
    ],
    &[
        (1.00, 71, 0.54),
        (3.00, 76, 0.40),
        (5.00, 66, 0.54),
        (7.50, 64, 0.52),
    ],
];

/// Render the complete 80-second loop at a requested sample rate.
///
/// Allocates one PCM buffer: `80 * sample_rate * 2 * size_of::<f32>()`
/// bytes (30.72 MB at 48 kHz; at most 61.44 MB). The fixed score and finite
/// voice tails bound CPU work. Generation belongs in asset preparation, never
/// in a realtime audio callback. Peaks are scaled to 0.22, leaving headroom.
///
/// Tails wrap into the beginning of the buffer. Loop the complete buffer;
/// adding an end fade or dropping the last frame would spoil the cyclic bed.
pub fn render_music(sample_rate: u32) -> Result<MusicPcm, MusicError> {
    if !(8_000..=96_000).contains(&sample_rate) {
        return Err(MusicError::UnsupportedSampleRate);
    }
    let sample_count = DURATION_SECONDS as usize * sample_rate as usize * CHANNELS;
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(sample_count)
        .map_err(|_| MusicError::AllocationFailed)?;
    samples.resize(sample_count, 0.0_f32);

    for (phrase, chord) in CHORDS.iter().enumerate() {
        let start = phrase as f64 * 10.0;
        for (voice, note) in chord.iter().enumerate() {
            add_pad(&mut samples, sample_rate, start - 2.0, *note, voice);
        }
        // A low, soft punctuation rather than a rhythmic accompaniment.
        add_key(
            &mut samples,
            sample_rate,
            start + 0.09,
            chord[1],
            0.36,
            -0.08,
        );
        for (index, &(beat, note, velocity)) in PHRASES[phrase].iter().enumerate() {
            let pan = if (phrase + index) % 2 == 0 {
                -0.22
            } else {
                0.22
            };
            add_key(
                &mut samples,
                sample_rate,
                start + beat * 1.25,
                note,
                velocity,
                pan,
            );
        }
    }

    let peak = samples
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    if peak > 0.0 {
        let gain = 0.22 / peak;
        for sample in &mut samples {
            *sample *= gain;
        }
    }
    Ok(MusicPcm {
        sample_rate,
        samples: samples.into_boxed_slice(),
    })
}

fn frequency(note: u8) -> f64 {
    440.0 * 2.0_f64.powf((note as f64 - 69.0) / 12.0)
}

fn smooth_edge(position: f64) -> f64 {
    (FRAC_PI_2 * position.clamp(0.0, 1.0)).sin().powi(2)
}

fn pan_gains(pan: f64) -> [f64; 2] {
    [((1.0 - pan) * 0.5).sqrt(), ((1.0 + pan) * 0.5).sqrt()]
}

fn start_frame(start: f64, sample_rate: u32, frames: usize) -> usize {
    ((start * sample_rate as f64).round() as i64).rem_euclid(frames as i64) as usize
}

fn mix(samples: &mut [f32], frame: usize, sample: f64, gains: [f64; 2]) {
    samples[frame * CHANNELS] += (sample * gains[0]) as f32;
    samples[frame * CHANNELS + 1] += (sample * gains[1]) as f32;
}

fn add_pad(samples: &mut [f32], rate: u32, start: f64, note: u8, voice: usize) {
    const LEVELS: [f64; 4] = [0.011, 0.009, 0.007, 0.006];
    const PANS: [f64; 4] = [-0.38, 0.34, -0.16, 0.18];
    let frames = samples.len() / CHANNELS;
    let first = start_frame(start, rate, frames);
    let hz = frequency(note);
    let gains = pan_gains(PANS[voice]);
    for i in 0..12 * rate as usize {
        let t = i as f64 / rate as f64;
        // Two-second raised-cosine crossfades keep harmony changes soft.
        let envelope = smooth_edge(t / 2.0) * smooth_edge((12.0 - t) / 2.0);
        let phase = TAU * hz * t;
        let tone = 0.5 * (phase * 0.9987).sin()
            + 0.5 * (phase * 1.0013).sin()
            + 0.10 * (phase * 2.0).sin();
        mix(
            samples,
            (first + i) % frames,
            tone * envelope * LEVELS[voice],
            gains,
        );
    }
}

fn add_key(samples: &mut [f32], rate: u32, start: f64, note: u8, velocity: f64, pan: f64) {
    let frames = samples.len() / CHANNELS;
    let first = start_frame(start, rate, frames);
    let hz = frequency(note);
    let direct = pan_gains(pan);
    let reflected = pan_gains(-pan * 1.5);
    // Three fixed low-level reflections, with no feedback or additional buffers.
    let taps = [(0.113, 0.15), (0.173, 0.09), (0.283, 0.05)]
        .map(|(delay, gain)| ((delay * rate as f64).round() as usize, gain));
    for i in 0..8 * rate as usize {
        let t = i as f64 / rate as f64;
        let phase = TAU * hz * t;
        let envelope = smooth_edge(t / 0.024) * smooth_edge((8.0 - t) / 1.2);
        let tone = (-t / 2.15).exp() * phase.sin()
            + 0.24 * (-t / 1.15).exp() * (phase * 2.0).sin()
            + 0.075 * (-t / 0.65).exp() * (phase * 3.0).sin()
            + 0.030 * (-t / 0.40).exp() * (phase * 4.01).sin();
        let value = 0.085 * velocity * envelope * tone;
        mix(samples, (first + i) % frames, value, direct);
        for &(delay, gain) in &taps {
            mix(
                samples,
                (first + i + delay) % frames,
                value * gain,
                reflected,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unbounded_or_invalid_rates() {
        for rate in [0, 7_999, 96_001, u32::MAX] {
            assert!(matches!(
                render_music(rate),
                Err(MusicError::UnsupportedSampleRate)
            ));
        }
    }

    #[test]
    fn output_is_finite_bounded_and_repeatable() {
        let first = render_music(8_000).unwrap();
        let second = render_music(8_000).unwrap();
        assert_eq!(first.sample_rate(), 8_000);
        assert_eq!(first.frames(), 640_000);
        assert_eq!(first.samples(), second.samples());
        assert!(first
            .samples()
            .iter()
            .all(|s| s.is_finite() && s.abs() <= 0.220_001));
        let mean =
            first.samples().iter().map(|s| *s as f64).sum::<f64>() / first.samples().len() as f64;
        assert!(mean.abs() < 0.000_1, "unexpected DC bias: {mean}");
    }
}
