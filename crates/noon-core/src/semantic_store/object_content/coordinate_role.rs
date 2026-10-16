//! Persistent scalar range and reversible mapping on an ordinary NumberLine shaft.
//!
//! The range is semantic metadata, not a renderer type or a frontend cache.
//! Canonical bits provide reflexive Eq/Hash even before admission rejects NaNs.

/// Reversible scalar mapping for an ordinary NumberLine coordinate axis.
///
/// The compact tagged representation keeps the coordinate definition copyable
/// with semantic object state for shared geometry and semantic admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticNumberLineScale {
    kind: u8,
    parameter_bits: u64,
}

impl SemanticNumberLineScale {
    const LINEAR: u8 = 0;
    const LOG: u8 = 1;

    pub fn linear(scale_factor: f64) -> Self {
        Self {
            kind: Self::LINEAR,
            parameter_bits: Self::canonical_bits(scale_factor),
        }
    }

    pub fn log(base: f64) -> Self {
        Self {
            kind: Self::LOG,
            parameter_bits: Self::canonical_bits(base),
        }
    }

    pub fn scale_factor(self) -> Option<f64> {
        (self.kind == Self::LINEAR).then(|| f64::from_bits(self.parameter_bits))
    }

    pub fn base(self) -> Option<f64> {
        (self.kind == Self::LOG).then(|| f64::from_bits(self.parameter_bits))
    }

    pub fn map(self, coordinate: f64) -> Option<f64> {
        if !coordinate.is_finite() || !self.is_valid() {
            return None;
        }
        let value = match self.kind {
            Self::LINEAR => coordinate * f64::from_bits(self.parameter_bits),
            Self::LOG => f64::from_bits(self.parameter_bits).powf(coordinate),
            _ => return None,
        };
        (value.is_finite() && (self.kind != Self::LOG || value > 0.0)).then_some(value)
    }

    pub fn unmap(self, value: f64) -> Option<f64> {
        if !value.is_finite() || !self.is_valid() {
            return None;
        }
        let coordinate = match self.kind {
            Self::LINEAR => value / f64::from_bits(self.parameter_bits),
            Self::LOG if value > 0.0 => value.log(f64::from_bits(self.parameter_bits)),
            Self::LOG => return None,
            _ => return None,
        };
        coordinate.is_finite().then_some(coordinate)
    }

    pub fn is_valid(self) -> bool {
        let parameter = f64::from_bits(self.parameter_bits);
        match self.kind {
            Self::LINEAR => parameter.is_finite() && parameter != 0.0,
            Self::LOG => parameter.is_finite() && parameter > 0.0 && parameter != 1.0,
            _ => false,
        }
    }

    fn canonical_bits(value: f64) -> u64 {
        if value == 0.0 {
            0
        } else {
            value.to_bits()
        }
    }
}

impl Default for SemanticNumberLineScale {
    fn default() -> Self {
        Self::linear(1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticNumberLineRole {
    range_bits: [u64; 3],
    scale: SemanticNumberLineScale,
}

impl SemanticNumberLineRole {
    pub fn new(range: [f64; 3]) -> Self {
        Self::with_scale(range, SemanticNumberLineScale::default())
    }

    pub fn with_scale(range: [f64; 3], scale: SemanticNumberLineScale) -> Self {
        Self {
            range_bits: range.map(|value| if value == 0.0 { 0 } else { value.to_bits() }),
            scale,
        }
    }

    pub fn range(self) -> [f64; 3] {
        self.range_bits.map(f64::from_bits)
    }

    pub fn scale(self) -> SemanticNumberLineScale {
        self.scale
    }

    pub fn is_valid(self) -> bool {
        let [start, end, step] = self.range();
        let mapped_start = self.scale.map(start);
        let mapped_end = self.scale.map(end);
        start.is_finite()
            && end.is_finite()
            && step.is_finite()
            && start < end
            && step > 0.0
            && (end - start).is_finite()
            && self.scale.is_valid()
            && mapped_start.is_some_and(f64::is_finite)
            && mapped_end.is_some_and(f64::is_finite)
            && mapped_start != mapped_end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticObjectRole, SemanticObjectState, StoredGeometry, Vec2};

    #[test]
    fn role_validates_range_and_canonicalizes_zero() {
        assert_eq!(
            SemanticNumberLineRole::new([-0.0, 1.0, 0.1]),
            SemanticNumberLineRole::new([0.0, 1.0, 0.1])
        );
        for range in [[1.0, 0.0, 1.0], [0.0, 1.0, 0.0], [0.0, f64::NAN, 1.0]] {
            let role = SemanticNumberLineRole::new(range);
            assert_eq!(role, role);
            assert!(!role.is_valid());
        }
    }

    #[test]
    fn scaled_roles_keep_mapping_and_require_a_finite_invertible_domain() {
        let role = SemanticNumberLineRole::with_scale(
            [-2.0, 2.0, 1.0],
            SemanticNumberLineScale::log(10.0),
        );
        assert!(role.is_valid());
        assert_eq!(role.scale().base(), Some(10.0));
        assert!(!SemanticNumberLineRole::with_scale(
            [300.0, 400.0, 1.0],
            SemanticNumberLineScale::log(10.0),
        )
        .is_valid());
        assert!(!SemanticNumberLineRole::with_scale(
            [-400.0, -300.0, 1.0],
            SemanticNumberLineScale::log(10.0),
        )
        .is_valid());
        assert!(!SemanticNumberLineScale::log(1.0).is_valid());
        assert!(!SemanticNumberLineScale::linear(0.0).is_valid());
    }

    #[test]
    fn copying_visual_state_preserves_receiver_coordinate_definition() {
        let mut source = SemanticObjectState::new(StoredGeometry::Line {
            start: Vec2::ZERO,
            end: Vec2::new(1.0, 0.0),
        });
        let role = SemanticObjectRole::NumberLine(SemanticNumberLineRole::new([2.0, 6.0, 1.0]));
        source.set_role(role);
        let target = SemanticObjectState::new(StoredGeometry::Line {
            start: Vec2::new(3.0, 1.0),
            end: Vec2::new(7.0, 1.0),
        });
        assert_eq!(source.clone().role(), role);
        let replaced = source.with_visual_state_from(&target);
        assert_eq!(replaced.content, target.content);
        assert_eq!(replaced.role(), role);
    }
}
