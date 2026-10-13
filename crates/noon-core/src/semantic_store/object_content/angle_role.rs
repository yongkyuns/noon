//! Nominal Angle value remains authored metadata on an ordinary retained path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticAngleRole {
    value_bits: Option<u64>,
}

impl SemanticAngleRole {
    pub fn new(value: Option<f64>) -> Self {
        Self {
            value_bits: value.map(|v| if v == 0.0 { 0 } else { v.to_bits() }),
        }
    }
    pub fn radians(self) -> Option<f64> {
        self.value_bits.map(f64::from_bits)
    }
    pub fn is_valid(self) -> bool {
        self.radians().is_none_or(f64::is_finite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nominal_metadata_is_reflexive_canonical_and_finite_on_admission() {
        assert_eq!(
            SemanticAngleRole::new(Some(-0.0)),
            SemanticAngleRole::new(Some(0.0))
        );
        let bad = SemanticAngleRole::new(Some(f64::NAN));
        assert_eq!(bad, bad);
        assert!(!bad.is_valid());
        assert!(SemanticAngleRole::new(None).is_valid());
        assert!(SemanticAngleRole::new(Some(std::f64::consts::TAU)).is_valid());
    }
}
