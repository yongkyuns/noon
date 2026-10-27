//! Inert deterministic mask parameters. The shared runtime supplies progress;
//! this value has no clock, attachment identity, resource or scene membership.

/// Seeded value-noise dissolve over normalized source bounds. Zero progress is
/// exactly the original premultiplied input; one is exactly transparent. The
/// version-one field has sixteen lattice cells per normalized source axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoiseDissolve {
    seed: u32,
    softness: f64,
    progress: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseDissolveError {
    InvalidSoftness,
    InvalidProgress,
}
impl std::fmt::Display for NoiseDissolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidSoftness => "dissolve softness must be finite and in [0, 1]",
            Self::InvalidProgress => "dissolve progress must be finite and in [0, 1]",
        })
    }
}
impl std::error::Error for NoiseDissolveError {}
impl Default for NoiseDissolve {
    fn default() -> Self {
        Self {
            seed: 0,
            softness: 0.08,
            progress: 0.0,
        }
    }
}
impl NoiseDissolve {
    pub fn new(seed: u32, softness: f64) -> Result<Self, NoiseDissolveError> {
        if !softness.is_finite() || !(0.0..=1.0).contains(&softness) {
            return Err(NoiseDissolveError::InvalidSoftness);
        }
        Ok(Self {
            seed,
            softness,
            progress: 0.0,
        })
    }
    pub fn with_progress(mut self, progress: f64) -> Result<Self, NoiseDissolveError> {
        if !progress.is_finite() || !(0.0..=1.0).contains(&progress) {
            return Err(NoiseDissolveError::InvalidProgress);
        }
        self.progress = progress;
        Ok(self)
    }
    pub const fn seed(self) -> u32 {
        self.seed
    }
    pub const fn softness(self) -> f64 {
        self.softness
    }
    pub const fn progress(self) -> f64 {
        self.progress
    }
    pub fn is_neutral(self) -> bool {
        self.progress == 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_parameters_preserve_precision_and_reject_nonfinite_values() {
        let softness = 0.0800000000000123;
        let progress = 0.4000000000000123;
        let value = NoiseDissolve::new(u32::MAX, softness)
            .unwrap()
            .with_progress(progress)
            .unwrap();
        assert_eq!(
            (value.seed(), value.softness(), value.progress()),
            (u32::MAX, softness, progress)
        );
        for invalid in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -f64::EPSILON,
            1.0 + f64::EPSILON,
        ] {
            assert_eq!(
                NoiseDissolve::new(0, invalid),
                Err(NoiseDissolveError::InvalidSoftness)
            );
            assert_eq!(
                value.with_progress(invalid),
                Err(NoiseDissolveError::InvalidProgress)
            );
        }
        for softness in [0.0, 1.0] {
            let full = NoiseDissolve::new(7, softness).unwrap();
            assert!(full.is_neutral());
            assert!(!full.with_progress(1.0).unwrap().is_neutral());
        }
    }
}
