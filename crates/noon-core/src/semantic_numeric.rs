use crate::Vec2;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// Authoring/semantic-space coordinate with f64 precision and explicit z.
///
/// Runtime and renderer storage may remain compact `f32`/2D. Call
/// [`SemanticVec3::try_lower_xy`] at the intentional semantic-to-runtime boundary
/// instead of relying on implicit narrowing in frontend or geometry code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SemanticVec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl SemanticVec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const ONE: Self = Self::new(1.0, 1.0, 1.0);

    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn length(self) -> f64 {
        self.x.hypot(self.y).hypot(self.z)
    }

    pub fn normalized(self) -> Option<Self> {
        let length = self.length();
        (length > 0.0 && length.is_finite()).then(|| self / length)
    }

    /// Lower semantic x/y coordinates to the current compact runtime/render type.
    ///
    /// This conversion is deliberately explicit because it is lossy even when the
    /// values are representable. Non-finite or out-of-range inputs are rejected
    /// rather than silently producing invalid runtime state.
    pub fn try_lower_xy(self) -> Result<Vec2, SemanticLoweringError> {
        Ok(Vec2::new(
            lower_component("x", self.x)?,
            lower_component("y", self.y)?,
        ))
    }
}

impl Add for SemanticVec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl AddAssign for SemanticVec3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for SemanticVec3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl SubAssign for SemanticVec3 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Neg for SemanticVec3 {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self::new(-self.x, -self.y, -self.z)
    }
}

impl Mul<f64> for SemanticVec3 {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl Mul<SemanticVec3> for f64 {
    type Output = SemanticVec3;

    fn mul(self, rhs: SemanticVec3) -> Self::Output {
        rhs * self
    }
}

impl Div<f64> for SemanticVec3 {
    type Output = Self;

    fn div(self, rhs: f64) -> Self::Output {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SemanticLoweringError {
    NonFinite { component: &'static str, value: f64 },
    OutOfRange { component: &'static str, value: f64 },
}

impl fmt::Display for SemanticLoweringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::NonFinite { component, value } => {
                write!(formatter, "semantic {component} coordinate must be finite: {value}")
            }
            Self::OutOfRange { component, value } => write!(
                formatter,
                "semantic {component} coordinate is outside the f32 runtime range: {value}"
            ),
        }
    }
}

impl std::error::Error for SemanticLoweringError {}

fn lower_component(component: &'static str, value: f64) -> Result<f32, SemanticLoweringError> {
    if !value.is_finite() {
        return Err(SemanticLoweringError::NonFinite { component, value });
    }
    if value < f32::MIN as f64 || value > f32::MAX as f64 {
        return Err(SemanticLoweringError::OutOfRange { component, value });
    }
    Ok(value as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_vec3_preserves_precision_until_explicit_lowering() {
        let a = SemanticVec3::new(2.0, -1.0, 0.0);
        let b = SemanticVec3::new(2.0 + 1.0e-12, -1.0, 0.0);

        assert_ne!(a.x, b.x, "semantic f64 coordinates must preserve sub-f32 differences");
        assert_eq!(
            a.try_lower_xy().unwrap().x,
            b.try_lower_xy().unwrap().x,
            "loss is expected only at the explicit compact-runtime boundary"
        );
    }

    #[test]
    fn semantic_vec3_supports_three_component_authoring_math() {
        let value = SemanticVec3::new(3.0, 4.0, 12.0);
        assert_eq!(value.length(), 13.0);
        let normalized = value.normalized().unwrap();
        assert!((normalized.length() - 1.0).abs() <= f64::EPSILON * 4.0);
        assert_eq!(SemanticVec3::ZERO + value - value, SemanticVec3::ZERO);
    }

    #[test]
    fn lowering_rejects_invalid_runtime_coordinates() {
        assert!(matches!(
            SemanticVec3::new(f64::NAN, 0.0, 0.0).try_lower_xy(),
            Err(SemanticLoweringError::NonFinite { component: "x", .. })
        ));
        assert!(matches!(
            SemanticVec3::new(f32::MAX as f64 * 2.0, 0.0, 0.0).try_lower_xy(),
            Err(SemanticLoweringError::OutOfRange { component: "x", .. })
        ));
    }
}
