//! Retained array dimensions and axis-zero concatenation, independent of hosts.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<usize>", into = "Vec<usize>")]
pub struct PointArrayShape(Vec<usize>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PointArrayShapeError {
    NoAxes,
    SizeOverflow,
    ExpectedZeroRows,
    InvalidDimension,
    RankMismatch {
        left: usize,
        right: usize,
    },
    AxisMismatch {
        axis: usize,
        left: usize,
        right: usize,
    },
}

impl PointArrayShape {
    pub fn new(dimensions: Vec<usize>) -> Result<Self, PointArrayShapeError> {
        if dimensions.is_empty() {
            return Err(PointArrayShapeError::NoAxes);
        }
        dimensions.iter().try_fold(1usize, |size, &dimension| {
            size.checked_mul(dimension)
                .ok_or(PointArrayShapeError::SizeOverflow)
        })?;
        Ok(Self(dimensions))
    }

    pub fn xyz(rows: usize) -> Result<Self, PointArrayShapeError> {
        Self::new(vec![rows, 3])
    }

    pub fn dimensions(&self) -> &[usize] {
        &self.0
    }

    pub fn rows(&self) -> usize {
        self.0[0]
    }

    /// NumPy append(axis=0) checks rank and every unselected axis, including
    /// when one or both arrays contain no coordinate values.
    pub fn concatenated(&self, other: &Self) -> Result<Self, PointArrayShapeError> {
        if self.0.len() != other.0.len() {
            return Err(PointArrayShapeError::RankMismatch {
                left: self.0.len(),
                right: other.0.len(),
            });
        }
        for (axis, (&left, &right)) in self.0.iter().zip(&other.0).enumerate().skip(1) {
            if left != right {
                return Err(PointArrayShapeError::AxisMismatch { axis, left, right });
            }
        }
        let mut dimensions = self.0.clone();
        dimensions[0] = dimensions[0]
            .checked_add(other.rows())
            .ok_or(PointArrayShapeError::SizeOverflow)?;
        Self::new(dimensions)
    }
}

impl Default for PointArrayShape {
    fn default() -> Self {
        Self(vec![0, 3])
    }
}

impl TryFrom<Vec<usize>> for PointArrayShape {
    type Error = PointArrayShapeError;
    fn try_from(dimensions: Vec<usize>) -> Result<Self, Self::Error> {
        Self::new(dimensions)
    }
}

impl From<PointArrayShape> for Vec<usize> {
    fn from(shape: PointArrayShape) -> Self {
        shape.0
    }
}

impl std::fmt::Display for PointArrayShapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAxes => f.write_str("point arrays require at least one axis"),
            Self::SizeOverflow => f.write_str("point array shape is too large"),
            Self::ExpectedZeroRows => f.write_str("empty point arrays must have zero rows"),
            Self::InvalidDimension => f.write_str("point array dimensions must be nonnegative representable integers"),
            Self::RankMismatch { left, right } => write!(f,
                "all the input arrays must have same number of dimensions, but the array at index 0 has {left} dimension(s) and the array at index 1 has {right} dimension(s)"),
            Self::AxisMismatch { axis, left, right } => write!(f,
                "all the input array dimensions except for the concatenation axis must match exactly, but along dimension {axis}, the array at index 0 has size {left} and the array at index 1 has size {right}"),
        }
    }
}
impl std::error::Error for PointArrayShapeError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concatenation_keeps_empty_rank_and_checks_other_axes() {
        let flat = PointArrayShape::new(vec![0]).unwrap();
        let xy = PointArrayShape::new(vec![0, 2]).unwrap();
        let xyz = PointArrayShape::xyz(4).unwrap();
        assert_eq!(flat.concatenated(&flat).unwrap(), flat);
        assert_eq!(PointArrayShape::default().concatenated(&xyz).unwrap(), xyz);
        assert_eq!(
            flat.concatenated(&xyz),
            Err(PointArrayShapeError::RankMismatch { left: 1, right: 2 })
        );
        assert_eq!(
            xyz.concatenated(&flat),
            Err(PointArrayShapeError::RankMismatch { left: 2, right: 1 })
        );
        assert_eq!(
            xyz.concatenated(&xy),
            Err(PointArrayShapeError::AxisMismatch {
                axis: 1,
                left: 3,
                right: 2
            })
        );
        assert!(PointArrayShape::new(vec![]).is_err());
        assert!(PointArrayShape::new(vec![usize::MAX, 2]).is_err());
    }
}
