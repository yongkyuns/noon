//! Checked colored material emission, independent of painted coverage.
/// Additional linear radiance in units of the decoded base paint. A strength
/// of zero is neutral; three makes the material four times as bright. Alpha
/// and object/group opacity retain their ordinary meaning.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Emission(f64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmissionError {
    InvalidStrength,
    InvalidProgress,
}
impl std::fmt::Display for EmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidStrength => "emission strength must be finite and in [0, 63]",
            Self::InvalidProgress => "emission interpolation progress must be finite",
        })
    }
}
impl std::error::Error for EmissionError {}
impl Emission {
    pub const NONE: Self = Self(0.0);
    pub fn new(strength: f64) -> Result<Self, EmissionError> {
        if !strength.is_finite() || !(0.0..=63.0).contains(&strength) {
            return Err(EmissionError::InvalidStrength);
        }
        Ok(Self(strength))
    }
    /// Material interpolation clips easing overshoot to the endpoint interval.
    pub fn interpolate(self, to: Self, progress: f64) -> Result<Self, EmissionError> {
        if !progress.is_finite() {
            return Err(EmissionError::InvalidProgress);
        }
        let progress = progress.clamp(0.0, 1.0);
        if progress == 0.0 {
            return Ok(self);
        }
        if progress == 1.0 {
            return Ok(to);
        }
        Self::new((self.0 * (1.0 - progress) + to.0 * progress).clamp(0.0, 63.0))
    }
    pub const fn strength(self) -> f64 {
        self.0
    }
    pub const fn is_neutral(&self) -> bool {
        self.0 == 0.0
    }
    pub const fn radiance_multiplier(self) -> f64 {
        1.0 + self.0
    }
}
impl TryFrom<f64> for Emission {
    type Error = EmissionError;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<Emission> for f64 {
    fn from(value: Emission) -> f64 {
        value.strength()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_and_authored_precision_are_checked() {
        assert_eq!(Emission::NONE.radiance_multiplier(), 1.0);
        assert_eq!(Emission::new(63.0).unwrap().radiance_multiplier(), 64.0);
        let value = 3.0 + f64::EPSILON * 4.0;
        assert_eq!(Emission::new(value).unwrap().strength(), value);
        for invalid in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -f64::EPSILON,
            63.0001,
        ] {
            assert!(Emission::new(invalid).is_err());
        }
    }
    #[test]
    fn external_codecs_and_interpolation_cannot_bypass_the_numeric_contract() {
        use serde::de::value::{Error, F64Deserializer};
        use serde::Deserialize;
        for invalid in [64.0, -1.0, f64::NAN] {
            assert!(Emission::deserialize(F64Deserializer::<Error>::new(invalid)).is_err());
        }
        let value = Emission::new(3.000000000000001).unwrap();
        assert_eq!(
            Emission::deserialize(F64Deserializer::<Error>::new(value.strength())).unwrap(),
            value
        );
        assert!(value.interpolate(Emission::NONE, f64::NAN).is_err());
        assert_eq!(value.interpolate(Emission::NONE, -1.0).unwrap(), value);
        assert_eq!(
            value.interpolate(Emission::NONE, 2.0).unwrap(),
            Emission::NONE
        );
    }
}
