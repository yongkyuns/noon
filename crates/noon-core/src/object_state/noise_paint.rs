//! Inert seeded value-noise paint; no scene identity, resource or clock.
use crate::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoisePaintError {
    InvalidColor,
    InvalidFrequency,
    InvalidPhase,
    InvalidSamplePoint,
    InvalidFootprint,
    InvalidProgress,
    DifferentSeed,
}

impl std::fmt::Display for NoisePaintError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidColor => "noise colors require finite RGBA channels in [0, 1]",
            Self::InvalidFrequency => "noise frequency requires two positive finite cell counts",
            Self::InvalidPhase => "noise phase requires two finite lattice coordinates",
            Self::InvalidFootprint => {
                "noise filter widths require finite nonnegative lattice lengths"
            }
            Self::InvalidProgress => "noise progress must be finite and in [0, 1]",
            Self::DifferentSeed => {
                "noise animation requires matching seeds; seed changes are immediate edits"
            }
            Self::InvalidSamplePoint => "noise sample coordinates must be finite",
        })
    }
}
impl std::error::Error for NoisePaintError {}

/// A deterministic two-color field in the normalized local geometry box.
///
/// Frequency is the number of lattice cells across each box axis. Phase shifts
/// that lattice in cell coordinates. Input points clamp to the local box before
/// frequency/phase mapping. Integer lattice coordinates wrap every 65,536 cells;
/// the fixed u32 hash and quintic interpolation below define the field, rather
/// than a platform random-number generator. Static values have no time input.
///
/// Colors interpolate in premultiplied encoded-sRGB; sampling includes no shape
/// coverage, fill multiplier or final object opacity. The filtered sampler
/// defines a bounded box/minification profile; a renderer supplies its physical
/// pixel footprint. Shared authoring/lowering owns attachment admission.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "NoisePaintInput")]
pub struct NoisePaint {
    seed: u32,
    frequency: [f64; 2],
    phase: [f64; 2],
    colors: [Color; 2],
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NoisePaintInput {
    seed: u32,
    frequency: [f64; 2],
    phase: [f64; 2],
    colors: [Color; 2],
}

impl TryFrom<NoisePaintInput> for NoisePaint {
    type Error = NoisePaintError;
    fn try_from(value: NoisePaintInput) -> Result<Self, Self::Error> {
        Self::new(value.seed, value.frequency, value.phase, value.colors)
    }
}

impl NoisePaint {
    pub fn new(
        seed: u32,
        frequency: [f64; 2],
        phase: [f64; 2],
        colors: [Color; 2],
    ) -> Result<Self, NoisePaintError> {
        if colors.iter().any(|c| {
            [c.red, c.green, c.blue, c.alpha]
                .into_iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
        }) {
            return Err(NoisePaintError::InvalidColor);
        }
        if frequency.into_iter().any(|v| !v.is_finite() || v <= 0.0) {
            return Err(NoisePaintError::InvalidFrequency);
        }
        if !phase.into_iter().all(f64::is_finite) {
            return Err(NoisePaintError::InvalidPhase);
        }
        Ok(Self {
            seed,
            frequency,
            phase,
            colors,
        })
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
    pub const fn colors(&self) -> [Color; 2] {
        self.colors
    }

    /// Split the periodic phase before GPU narrowing. Integer cells retain all
    /// sixteen period bits while the fraction keeps precision near the seam.
    pub fn phase_parts(&self) -> ([u32; 2], [f64; 2]) {
        let wrapped = self.phase.map(|v| v.rem_euclid(65_536.0));
        // rem_euclid can round a tiny negative input up to the period itself.
        // Keep that rounded seam at cell zero, including before packing X/Y.
        (
            wrapped.map(|v| (v.floor() as u32) & 65_535),
            wrapped.map(f64::fract),
        )
    }

    pub fn interpolate(&self, to: &Self, alpha: f64) -> Result<Self, NoisePaintError> {
        if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
            return Err(NoisePaintError::InvalidProgress);
        }
        if self.seed != to.seed {
            return Err(NoisePaintError::DifferentSeed);
        }
        if alpha == 0.0 {
            return Ok(self.clone());
        }
        if alpha == 1.0 {
            return Ok(to.clone());
        }
        let frequency = std::array::from_fn(|i| {
            alpha.mul_add(to.frequency[i] - self.frequency[i], self.frequency[i])
        });
        let phase = std::array::from_fn(|i| {
            if self.phase[i].is_sign_positive() == to.phase[i].is_sign_positive() {
                alpha.mul_add(to.phase[i] - self.phase[i], self.phase[i])
            } else {
                self.phase[i] * (1.0 - alpha) + to.phase[i] * alpha
            }
        });
        let colors = std::array::from_fn(|i| {
            super::interpolate_premultiplied_color(self.colors[i], to.colors[i], alpha)
        });
        Self::new(self.seed, frequency, phase, colors)
    }

    pub fn sample_premultiplied(&self, point: [f64; 2]) -> Result<[f64; 4], NoisePaintError> {
        self.sample_filtered_premultiplied(point, [0.0; 2])
    }

    /// Lattice-aligned box reconstruction around the clamped sample center.
    /// Widths below 1/256 cell use point reconstruction. Wider boxes integrate
    /// quintic basis weights analytically, with at most nine lattice values.
    /// Above one cell, a cubic fade reaches the seed ensemble mean at two cells.
    /// This bounded coarse profile is explicit, not arbitrary alias-free filtering.
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
        let coarse = footprint[0].max(footprint[1]);
        if coarse >= 2.0 {
            return Ok(self.color_at(0.5));
        }
        let (phase_cell, phase_fraction) = self.phase_parts();
        let coordinates: [f64; 2] = std::array::from_fn(|axis| {
            (point[axis].clamp(0.0, 1.0) * self.frequency[axis] + phase_fraction[axis])
                .rem_euclid(65_536.0)
        });
        let axes: [(u32, [f64; 3]); 2] =
            std::array::from_fn(|axis| noise_weights(coordinates[axis], footprint[axis].min(1.0)));
        let mut value = 0.0;
        for x in 0..3 {
            for y in 0..3 {
                let ix = (phase_cell[0] + axes[0].0 + x as u32) & 65_535;
                let iy = (phase_cell[1] + axes[1].0 + y as u32) & 65_535;
                let lattice = f64::from(lattice_hash(self.seed, ix, iy) >> 16) / 65_535.0;
                value += lattice * axes[0].1[x] * axes[1].1[y];
            }
        }
        let u = (coarse - 1.0).clamp(0.0, 1.0);
        let fade = u * u * (3.0 - 2.0 * u);
        Ok(self.color_at((value * (1.0 - fade) + 0.5 * fade).clamp(0.0, 1.0)))
    }

    fn color_at(&self, value: f64) -> [f64; 4] {
        let colors = self.colors.map(|c| {
            let a = f64::from(c.alpha);
            [
                f64::from(c.red) * a,
                f64::from(c.green) * a,
                f64::from(c.blue) * a,
                a,
            ]
        });
        std::array::from_fn(|i| colors[0][i] * (1.0 - value) + colors[1][i] * value)
    }
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
