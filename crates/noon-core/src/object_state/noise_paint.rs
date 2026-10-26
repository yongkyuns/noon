//! Inert seeded value-noise paint; no scene identity, resource or clock.
use crate::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoisePaintError {
    InvalidColor,
    InvalidFrequency,
    InvalidPhase,
    InvalidSamplePoint,
}

impl std::fmt::Display for NoisePaintError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidColor => "noise colors require finite RGBA channels in [0, 1]",
            Self::InvalidFrequency => "noise frequency requires two positive finite cell counts",
            Self::InvalidPhase => "noise phase requires two finite lattice coordinates",
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
/// coverage, fill multiplier or final object opacity. This value defines neither
/// pixel-footprint antialiasing nor a renderer admission profile. Public Scene
/// and language-wrapper support require the subsequent shared vertical slice.
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

    pub fn sample_premultiplied(&self, point: [f64; 2]) -> Result<[f64; 4], NoisePaintError> {
        if !point.into_iter().all(f64::is_finite) {
            return Err(NoisePaintError::InvalidSamplePoint);
        }
        let coordinates: [f64; 2] = std::array::from_fn(|axis| {
            point[axis].clamp(0.0, 1.0) * self.frequency[axis]
                + self.phase[axis].rem_euclid(65_536.0)
        });
        let cell = coordinates.map(|v| v.floor().rem_euclid(65_536.0) as u32);
        let weights = coordinates.map(|v| {
            let t = v.fract();
            (t * t * t * (t * (t * 6.0 - 15.0) + 10.0)).clamp(0.0, 1.0)
        });
        let value = |x, y| f64::from(lattice_hash(self.seed, x, y) >> 16) / 65_535.0;
        let mix = |a: f64, b: f64, t: f64| a * (1.0 - t) + b * t;
        let next = cell.map(|v| (v + 1) & 65_535);
        let low = mix(value(cell[0], cell[1]), value(next[0], cell[1]), weights[0]);
        let high = mix(value(cell[0], next[1]), value(next[0], next[1]), weights[0]);
        let t = mix(low, high, weights[1]);
        let premultiplied = self.colors.map(|c| {
            let alpha = f64::from(c.alpha);
            [
                f64::from(c.red) * alpha,
                f64::from(c.green) * alpha,
                f64::from(c.blue) * alpha,
                alpha,
            ]
        });
        Ok(std::array::from_fn(|i| {
            mix(premultiplied[0][i], premultiplied[1][i], t)
        }))
    }
}

fn lattice_hash(seed: u32, x: u32, y: u32) -> u32 {
    let mut h = seed ^ x.wrapping_mul(0x9e37_79b9) ^ y.wrapping_mul(0x85eb_ca6b);
    h = (h ^ (h >> 16)).wrapping_mul(0x7feb_352d);
    h = (h ^ (h >> 15)).wrapping_mul(0x846c_a68b);
    h ^ (h >> 16)
}

#[cfg(test)]
mod tests;
