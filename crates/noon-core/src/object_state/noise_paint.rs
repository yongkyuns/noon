//! Inert seeded value-noise paint; no scene identity, resource or clock.
use crate::{Color, GradientError, GradientStop};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoisePaintError {
    InvalidColor,
    InvalidFrequency,
    InvalidPhase,
    InvalidSamplePoint,
    InvalidFootprint,
    InvalidProgress,
    InvalidOctaves,
    InvalidLacunarity,
    InvalidGain,
    DifferentSeed,
    DifferentDetail,
    DifferentStopCounts,
    Gradient(GradientError),
}

impl std::fmt::Display for NoisePaintError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Self::Gradient(error) = self {
            return error.fmt(formatter);
        }
        formatter.write_str(match self {
            Self::InvalidColor => "noise colors require finite RGBA channels in [0, 1]",
            Self::InvalidFrequency => "noise frequency requires positive finite cell counts at every octave",
            Self::InvalidPhase => "noise phase requires two finite lattice coordinates",
            Self::InvalidFootprint => "noise filter widths require finite nonnegative lattice lengths",
            Self::InvalidProgress => "noise progress must be finite and in [0, 1]",
            Self::InvalidOctaves => "fractal noise requires between one and eight octaves",
            Self::InvalidLacunarity => "fractal noise lacunarity must be finite and in [1, 4]",
            Self::InvalidGain => "fractal noise gain must be finite and in [0, 1]",
            Self::DifferentSeed => "noise animation requires matching seeds; seed changes are immediate edits",
            Self::DifferentDetail => "noise animation requires matching octave counts and lacunarity; topology changes are immediate edits",
            Self::DifferentStopCounts => "noise animation requires matching color-ramp stop counts",
            Self::InvalidSamplePoint => "noise sample coordinates must be finite",
            Self::Gradient(_) => unreachable!(),
        })
    }
}
impl std::error::Error for NoisePaintError {}
impl From<GradientError> for NoisePaintError {
    fn from(value: GradientError) -> Self {
        Self::Gradient(value)
    }
}

// The common two-color paint stays allocation-free, including cloning and
// interpolation. Larger ramps share immutable storage across phase updates.
// Both variants serialize as the same checked stop array at persistence edges.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(untagged)]
enum NoiseStops {
    Inline([GradientStop; 2]),
    Shared(Arc<[GradientStop]>),
}
impl std::ops::Deref for NoiseStops {
    type Target = [GradientStop];
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Inline(stops) => stops,
            Self::Shared(stops) => stops,
        }
    }
}
impl PartialEq for NoiseStops {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

/// A seeded local field with a premultiplied encoded-sRGB color ramp.
///
/// `new` retains the original one-scale/two-color appearance. `fractal` starts
/// with four octaves, doubled frequency and halved amplitude. Builders describe
/// the same immutable value; no programs, textures, scene identities or clocks
/// are allocated. Frequency and phase use the original normalized local box.
/// Every octave applies the same bounded physical-pixel reconstruction filter
/// before scalar accumulation; the color ramp is applied to that scalar result.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "NoisePaintInput")]
pub struct NoisePaint {
    seed: u32,
    frequency: [f64; 2],
    phase: [f64; 2],
    stops: NoiseStops,
    octaves: u8,
    lacunarity: f64,
    gain: f64,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NoisePaintInput {
    seed: u32,
    frequency: [f64; 2],
    phase: [f64; 2],
    stops: Vec<GradientStop>,
    octaves: u8,
    lacunarity: f64,
    gain: f64,
}
impl TryFrom<NoisePaintInput> for NoisePaint {
    type Error = NoisePaintError;
    fn try_from(v: NoisePaintInput) -> Result<Self, Self::Error> {
        Self::new(v.seed, v.frequency, v.phase, [Color::BLACK, Color::WHITE])?
            .with_detail(v.octaves, v.lacunarity, v.gain)?
            .with_stops(v.stops)
    }
}

impl NoisePaint {
    pub const MAX_OCTAVES: u8 = 8;

    pub fn new(
        seed: u32,
        frequency: [f64; 2],
        phase: [f64; 2],
        colors: [Color; 2],
    ) -> Result<Self, NoisePaintError> {
        let stops = NoiseStops::Inline([
            GradientStop::new(0.0, colors[0]).map_err(|_| NoisePaintError::InvalidColor)?,
            GradientStop::new(1.0, colors[1]).map_err(|_| NoisePaintError::InvalidColor)?,
        ]);
        Self {
            seed,
            frequency,
            phase,
            stops,
            octaves: 1,
            lacunarity: 2.0,
            gain: 0.5,
        }
        .checked()
    }

    /// Useful bounded cloud-like detail from just two colors.
    pub fn fractal(low: Color, high: Color) -> Result<Self, NoisePaintError> {
        Self::new(0, [8.0; 2], [0.0; 2], [low, high])?.with_detail(4, 2.0, 0.5)
    }
    pub fn with_seed(mut self, seed: u32) -> Self {
        self.seed = seed;
        self
    }
    pub fn with_frequency(mut self, frequency: [f64; 2]) -> Result<Self, NoisePaintError> {
        self.frequency = frequency;
        self.checked()
    }
    pub fn with_phase(mut self, phase: [f64; 2]) -> Result<Self, NoisePaintError> {
        self.phase = phase;
        self.checked()
    }
    pub fn with_detail(
        mut self,
        octaves: u8,
        lacunarity: f64,
        gain: f64,
    ) -> Result<Self, NoisePaintError> {
        self.octaves = octaves;
        self.lacunarity = lacunarity;
        self.gain = gain;
        self.checked()
    }
    pub fn with_stops(
        mut self,
        stops: impl IntoIterator<Item = GradientStop>,
    ) -> Result<Self, NoisePaintError> {
        let stops = super::gradient::checked_stops(stops)?;
        self.stops = if stops.len() == 2 {
            NoiseStops::Inline([stops[0], stops[1]])
        } else {
            NoiseStops::Shared(stops)
        };
        Ok(self)
    }
    pub(crate) fn with_constant_color(&self, color: Color) -> Result<Self, NoisePaintError> {
        let mut value = self.clone();
        value.stops = match &self.stops {
            NoiseStops::Inline(stops) => NoiseStops::Inline([
                GradientStop::new(stops[0].position(), color)?,
                GradientStop::new(stops[1].position(), color)?,
            ]),
            NoiseStops::Shared(stops) => NoiseStops::Shared(
                stops
                    .iter()
                    .map(|stop| GradientStop::new(stop.position(), color))
                    .collect::<Result<Arc<[_]>, _>>()?,
            ),
        };
        Ok(value)
    }
    fn checked(self) -> Result<Self, NoisePaintError> {
        if !(1..=Self::MAX_OCTAVES).contains(&self.octaves) {
            return Err(NoisePaintError::InvalidOctaves);
        }
        if !self.lacunarity.is_finite() || !(1.0..=4.0).contains(&self.lacunarity) {
            return Err(NoisePaintError::InvalidLacunarity);
        }
        if !self.gain.is_finite() || !(0.0..=1.0).contains(&self.gain) {
            return Err(NoisePaintError::InvalidGain);
        }
        let scale = self.lacunarity.powi(i32::from(self.octaves - 1));
        if self
            .frequency
            .into_iter()
            .any(|v| !v.is_finite() || v <= 0.0 || !(v * scale).is_finite())
        {
            return Err(NoisePaintError::InvalidFrequency);
        }
        if !self.phase.into_iter().all(f64::is_finite) {
            return Err(NoisePaintError::InvalidPhase);
        }
        Ok(self)
    }
    pub const fn seed(&self) -> u32 {
        self.seed
    }
    pub const fn frequency(&self) -> [f64; 2] {
        self.frequency
    }
    pub const fn phase(&self) -> [f64; 2] {
        self.phase
    }
    /// Boundary colors; the complete scalar mapping is available in `stops`.
    pub fn colors(&self) -> [Color; 2] {
        [
            self.stops[0].color(),
            self.stops[self.stops.len() - 1].color(),
        ]
    }
    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }
    pub const fn octaves(&self) -> u8 {
        self.octaves
    }
    pub const fn lacunarity(&self) -> f64 {
        self.lacunarity
    }
    pub const fn gain(&self) -> f64 {
        self.gain
    }
    pub fn highest_frequency(&self) -> [f64; 2] {
        let scale = self.lacunarity.powi(i32::from(self.octaves - 1));
        self.frequency.map(|v| v * scale)
    }

    /// Split periodic phase before narrowing, retaining the original seam rule.
    pub fn phase_parts(&self) -> ([u32; 2], [f64; 2]) {
        split_phase(self.phase)
    }

    /// Each octave scales phase with frequency, so phase animation translates
    /// all scales together. Reduce before multiplication to avoid f64 overflow.
    pub fn octave_phase_parts(&self, octave: u8) -> Option<([u32; 2], [f64; 2])> {
        if octave >= self.octaves {
            return None;
        }
        if octave == 0 {
            return Some(self.phase_parts());
        }
        let scale = self.lacunarity.powi(i32::from(octave));
        Some(split_phase(
            self.phase.map(|v| v.rem_euclid(65_536.0 / scale) * scale),
        ))
    }

    pub fn interpolate(&self, to: &Self, alpha: f64) -> Result<Self, NoisePaintError> {
        if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
            return Err(NoisePaintError::InvalidProgress);
        }
        if self.seed != to.seed {
            return Err(NoisePaintError::DifferentSeed);
        }
        if self.octaves != to.octaves || self.lacunarity != to.lacunarity {
            return Err(NoisePaintError::DifferentDetail);
        }
        if self.stops.len() != to.stops.len() {
            return Err(NoisePaintError::DifferentStopCounts);
        }
        if alpha == 0.0 {
            return Ok(self.clone());
        }
        if alpha == 1.0 {
            return Ok(to.clone());
        }
        let mix = |a: f64, b: f64| {
            if a.is_sign_positive() == b.is_sign_positive() {
                alpha.mul_add(b - a, a)
            } else {
                a * (1.0 - alpha) + b * alpha
            }
        };
        let frequency = std::array::from_fn(|i| mix(self.frequency[i], to.frequency[i]));
        let phase = std::array::from_fn(|i| mix(self.phase[i], to.phase[i]));
        let interpolate_stop = |a: GradientStop, b: GradientStop| {
            GradientStop::new(
                mix(a.position(), b.position()),
                super::interpolate_premultiplied_color(a.color(), b.color(), alpha),
            )
        };
        let stops = if self.stops == to.stops {
            self.stops.clone()
        } else if let (NoiseStops::Inline(a), NoiseStops::Inline(b)) = (&self.stops, &to.stops) {
            NoiseStops::Inline([interpolate_stop(a[0], b[0])?, interpolate_stop(a[1], b[1])?])
        } else {
            NoiseStops::Shared(
                self.stops
                    .iter()
                    .zip(to.stops.iter())
                    .map(|(a, b)| interpolate_stop(*a, *b))
                    .collect::<Result<Arc<[_]>, _>>()?,
            )
        };
        Self {
            seed: self.seed,
            frequency,
            phase,
            stops,
            octaves: self.octaves,
            lacunarity: self.lacunarity,
            gain: mix(self.gain, to.gain),
        }
        .checked()
    }
    pub fn sample_premultiplied(&self, point: [f64; 2]) -> Result<[f64; 4], NoisePaintError> {
        self.sample_filtered_premultiplied(point, [0.0; 2])
    }
    /// The footprint is measured in base-octave lattice units. Each octave
    /// scales that footprint with its frequency before the existing bounded
    /// quintic reconstruction. The normalized weighted field is mapped to the
    /// ramp afterward; this is not exact filtering of a nonlinear color ramp.
    pub fn sample_filtered_premultiplied(
        &self,
        point: [f64; 2],
        footprint: [f64; 2],
    ) -> Result<[f64; 4], NoisePaintError> {
        if !point.into_iter().all(f64::is_finite) {
            return Err(NoisePaintError::InvalidSamplePoint);
        }
        if footprint.into_iter().any(|v| !v.is_finite() || v < 0.0) {
            return Err(NoisePaintError::InvalidFootprint);
        }
        if self.octaves == 1 {
            return Ok(self.color_at(filtered_field(
                self.seed,
                point,
                self.frequency,
                self.phase_parts(),
                footprint,
            )));
        }
        let mut sum = 0.0;
        let mut weights = 0.0;
        let mut weight = 1.0;
        let mut scale = 1.0;
        for octave in 0..self.octaves {
            if weight == 0.0 {
                break;
            }
            let seed = self
                .seed
                .wrapping_add(u32::from(octave).wrapping_mul(0x9e37_79b9));
            let value = filtered_field(
                seed,
                point,
                self.frequency.map(|v| v * scale),
                self.octave_phase_parts(octave).expect("bounded octave"),
                footprint.map(|v| v * scale),
            );
            sum += weight * value;
            weights += weight;
            scale *= self.lacunarity;
            weight *= self.gain;
        }
        Ok(self.color_at((sum / weights).clamp(0.0, 1.0)))
    }
    fn color_at(&self, value: f64) -> [f64; 4] {
        // Preserve the original two-color arithmetic exactly.
        if self.stops.len() == 2
            && self.stops[0].position() == 0.0
            && self.stops[1].position() == 1.0
        {
            let colors = self.colors().map(|c| {
                let a = f64::from(c.alpha);
                [
                    f64::from(c.red) * a,
                    f64::from(c.green) * a,
                    f64::from(c.blue) * a,
                    a,
                ]
            });
            return std::array::from_fn(|i| colors[0][i] * (1.0 - value) + colors[1][i] * value);
        }
        super::gradient::sample_stops(&self.stops, value)
    }
}
fn split_phase(phase: [f64; 2]) -> ([u32; 2], [f64; 2]) {
    let wrapped = phase.map(|v| v.rem_euclid(65_536.0));
    (
        wrapped.map(|v| (v.floor() as u32) & 65_535),
        wrapped.map(f64::fract),
    )
}
fn filtered_field(
    seed: u32,
    point: [f64; 2],
    frequency: [f64; 2],
    phase: ([u32; 2], [f64; 2]),
    footprint: [f64; 2],
) -> f64 {
    let coarse = footprint[0].max(footprint[1]);
    if coarse >= 2.0 {
        return 0.5;
    }
    let (phase_cell, phase_fraction) = phase;
    let coordinates: [f64; 2] = std::array::from_fn(|axis| {
        (point[axis].clamp(0.0, 1.0) * frequency[axis] + phase_fraction[axis]).rem_euclid(65_536.0)
    });
    let axes: [(u32, [f64; 3]); 2] =
        std::array::from_fn(|axis| noise_weights(coordinates[axis], footprint[axis].min(1.0)));
    let mut value = 0.0;
    for x in 0..3 {
        for y in 0..3 {
            let ix = (phase_cell[0] + axes[0].0 + x as u32) & 65_535;
            let iy = (phase_cell[1] + axes[1].0 + y as u32) & 65_535;
            value += f64::from(lattice_hash(seed, ix, iy) >> 16) / 65_535.0
                * axes[0].1[x]
                * axes[1].1[y];
        }
    }
    let u = (coarse - 1.0).clamp(0.0, 1.0);
    let fade = u * u * (3.0 - 2.0 * u);
    (value * (1.0 - fade) + 0.5 * fade).clamp(0.0, 1.0)
}

fn noise_weights(center: f64, width: f64) -> (u32, [f64; 3]) {
    let base;
    let mut weights;
    if width < 1.0 / 256.0 {
        base = center.floor();
        let t = center.fract();
        let smooth = (t * t * t * (t * (t * 6.0 - 15.0) + 10.0)).clamp(0.0, 1.0);
        weights = [1.0 - smooth, smooth, 0.0];
    } else {
        base = (center - width * 0.5).floor();
        let lo = center - width * 0.5 - base;
        let hi = center + width * 0.5 - base;
        let integral = |t: f64| t * t * t * t * (t * t - 3.0 * t + 2.5);
        let a = lo.clamp(0.0, 1.0);
        let b = hi.clamp(0.0, 1.0);
        let c = (lo - 1.0).clamp(0.0, 1.0);
        let d = (hi - 1.0).clamp(0.0, 1.0);
        let first = integral(b) - integral(a);
        let second = integral(d) - integral(c);
        weights = [b - a - first, first + d - c - second, second].map(|v| (v / width).max(0.0));
        let sum: f64 = weights.iter().sum();
        weights = weights.map(|v| v / sum);
    }
    (base.rem_euclid(65_536.0) as u32, weights)
}

fn lattice_hash(seed: u32, x: u32, y: u32) -> u32 {
    let mut h = seed ^ x.wrapping_mul(0x9e37_79b9) ^ y.wrapping_mul(0x85eb_ca6b);
    h = (h ^ (h >> 16)).wrapping_mul(0x7feb_352d);
    h = (h ^ (h >> 15)).wrapping_mul(0x846c_a68b);
    h ^ (h >> 16)
}

#[cfg(test)]
mod tests;
