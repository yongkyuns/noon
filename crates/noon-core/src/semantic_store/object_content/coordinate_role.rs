//! Persistent scalar range on an ordinary NumberLine shaft.
//!
//! The range is semantic metadata, not a renderer type or a frontend cache.
//! Canonical bits provide reflexive Eq/Hash even before admission rejects NaNs.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SemanticNumberLineRole {
    range_bits: [u64; 3],
}

impl SemanticNumberLineRole {
    pub fn new(range: [f64; 3]) -> Self {
        Self {
            range_bits: range.map(|value| if value == 0.0 { 0 } else { value.to_bits() }),
        }
    }

    pub fn range(self) -> [f64; 3] {
        self.range_bits.map(f64::from_bits)
    }

    pub fn is_valid(self) -> bool {
        let [start, end, step] = self.range();
        start.is_finite()
            && end.is_finite()
            && step.is_finite()
            && start < end
            && step > 0.0
            && (end - start).is_finite()
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
