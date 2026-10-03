use crate::SemanticVec3;

/// Axis-aligned bounds of a mesh in its local coordinate system.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshBounds3D {
    pub min: SemanticVec3,
    pub max: SemanticVec3,
}

/// Immutable indexed triangle data validated before it enters a resource arena.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshResource {
    positions: Vec<SemanticVec3>,
    normals: Option<Vec<SemanticVec3>>,
    indices: Vec<u32>,
    bounds: MeshBounds3D,
}

impl MeshResource {
    pub fn new(
        positions: Vec<SemanticVec3>,
        normals: Option<Vec<SemanticVec3>>,
        indices: Vec<u32>,
    ) -> Result<Self, MeshResourceError> {
        if positions.is_empty() {
            return Err(MeshResourceError::EmptyPositions);
        }
        if positions.iter().any(|position| !position.is_finite()) {
            return Err(MeshResourceError::NonFinitePosition);
        }
        if let Some(normals) = &normals {
            if normals.len() != positions.len() {
                return Err(MeshResourceError::NormalCountMismatch {
                    positions: positions.len(),
                    normals: normals.len(),
                });
            }
            if normals.iter().any(|normal| !normal.is_finite()) {
                return Err(MeshResourceError::NonFiniteNormal);
            }
        }
        if indices.is_empty() || !indices.len().is_multiple_of(3) {
            return Err(MeshResourceError::InvalidTriangleIndexCount(indices.len()));
        }
        if let Some(&index) = indices
            .iter()
            .find(|&&index| index as usize >= positions.len())
        {
            return Err(MeshResourceError::IndexOutOfBounds {
                index,
                positions: positions.len(),
            });
        }

        let mut min = positions[0];
        let mut max = positions[0];
        for point in &positions[1..] {
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            min.z = min.z.min(point.z);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
            max.z = max.z.max(point.z);
        }
        Ok(Self {
            positions,
            normals,
            indices,
            bounds: MeshBounds3D { min, max },
        })
    }

    pub fn positions(&self) -> &[SemanticVec3] {
        &self.positions
    }
    pub fn normals(&self) -> Option<&[SemanticVec3]> {
        self.normals.as_deref()
    }
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }
    pub const fn bounds(&self) -> MeshBounds3D {
        self.bounds
    }

    /// Logical payload bytes retained by the resource, excluding allocator overhead.
    pub fn retained_bytes(&self) -> usize {
        self.positions.len() * std::mem::size_of::<SemanticVec3>()
            + self.normals.as_ref().map_or(0, |values| {
                values.len() * std::mem::size_of::<SemanticVec3>()
            })
            + self.indices.len() * std::mem::size_of::<u32>()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshResourceError {
    EmptyPositions,
    NonFinitePosition,
    NonFiniteNormal,
    NormalCountMismatch { positions: usize, normals: usize },
    InvalidTriangleIndexCount(usize),
    IndexOutOfBounds { index: u32, positions: usize },
}

impl std::fmt::Display for MeshResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPositions => f.write_str("mesh must contain positions"),
            Self::NonFinitePosition => f.write_str("mesh contains a non-finite position"),
            Self::NonFiniteNormal => f.write_str("mesh contains a non-finite normal"),
            Self::NormalCountMismatch { positions, normals } => {
                write!(f, "mesh has {positions} positions but {normals} normals")
            }
            Self::InvalidTriangleIndexCount(count) => write!(
                f,
                "triangle mesh index count must be a non-zero multiple of three, got {count}"
            ),
            Self::IndexOutOfBounds { index, positions } => {
                write!(f, "mesh index {index} exceeds position count {positions}")
            }
        }
    }
}
impl std::error::Error for MeshResourceError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> MeshResource {
        MeshResource::new(
            vec![
                SemanticVec3::new(-1.0, 2.0, 0.5),
                SemanticVec3::new(3.0, -4.0, 2.0),
                SemanticVec3::new(0.0, 1.0, -5.0),
            ],
            None,
            vec![0, 1, 2],
        )
        .unwrap()
    }

    #[test]
    fn validates_payload_and_computes_three_dimensional_bounds() {
        let mesh = valid();
        assert_eq!(
            mesh.bounds(),
            MeshBounds3D {
                min: SemanticVec3::new(-1.0, -4.0, -5.0),
                max: SemanticVec3::new(3.0, 2.0, 2.0)
            }
        );
        assert_eq!(mesh.positions().len(), 3);
        assert_eq!(mesh.indices(), &[0, 1, 2]);
        assert_eq!(
            mesh.retained_bytes(),
            3 * std::mem::size_of::<SemanticVec3>() + 3 * std::mem::size_of::<u32>()
        );
    }

    #[test]
    fn rejects_non_finite_values_and_invalid_cardinality_and_indices() {
        assert_eq!(
            MeshResource::new(
                vec![SemanticVec3::new(f64::NAN, 0.0, 0.0)],
                None,
                vec![0, 0, 0]
            ),
            Err(MeshResourceError::NonFinitePosition)
        );
        assert_eq!(
            MeshResource::new(
                vec![SemanticVec3::ZERO; 3],
                Some(vec![SemanticVec3::ZERO]),
                vec![0, 1, 2]
            ),
            Err(MeshResourceError::NormalCountMismatch {
                positions: 3,
                normals: 1
            })
        );
        assert_eq!(
            MeshResource::new(vec![SemanticVec3::ZERO; 3], None, vec![0, 1]),
            Err(MeshResourceError::InvalidTriangleIndexCount(2))
        );
        assert_eq!(
            MeshResource::new(vec![SemanticVec3::ZERO; 3], None, vec![0, 1, 3]),
            Err(MeshResourceError::IndexOutOfBounds {
                index: 3,
                positions: 3
            })
        );
    }
}
