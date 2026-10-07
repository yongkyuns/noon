use crate::SemanticVec3;

/// Cairo-compatible per-cell gradient endpoints and corner-relative spans.
/// The renderer owns lighting and projection; this immutable geometry metadata
/// preserves mapped shading samples and optional perimeter controls.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CairoSurfaceAppearance {
    pub p0: SemanticVec3,
    pub p6: SemanticVec3,
    pub span_p3_p0: SemanticVec3,
    pub span_p12_p0: SemanticVec3,
    pub span_p9_p6: SemanticVec3,
    pub span_p3_p6: SemanticVec3,
    /// Optional cubic controls for the quad perimeter edges in mesh-position order:
    /// 0→1, 1→2, 2→3, and 3→0. Endpoints are the corresponding mesh positions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boundary_controls: Option<[[SemanticVec3; 2]; 4]>,
}

impl CairoSurfaceAppearance {
    pub fn is_finite(&self) -> bool {
        [
            self.p0,
            self.p6,
            self.span_p3_p0,
            self.span_p12_p0,
            self.span_p9_p6,
            self.span_p3_p6,
        ]
        .into_iter()
        .all(SemanticVec3::is_finite)
            && self
                .boundary_controls
                .is_none_or(|edges| edges.into_iter().flatten().all(SemanticVec3::is_finite))
    }

    /// Whether this appearance is valid for the given mesh payload. Boundary
    /// controls are only meaningful for a canonical four-vertex quad.
    pub fn is_valid_for_mesh(&self, positions: &[SemanticVec3], indices: &[u32]) -> bool {
        self.is_finite()
            && (self.boundary_controls.is_none()
                || Self::has_canonical_quad_topology(positions.len(), indices))
    }

    fn has_canonical_quad_topology(position_count: usize, indices: &[u32]) -> bool {
        if position_count != 4 || indices.len() != 6 {
            return false;
        }
        let mut triangles = [
            [indices[0], indices[1], indices[2]],
            [indices[3], indices[4], indices[5]],
        ];
        for triangle in &mut triangles {
            triangle.sort_unstable();
        }
        triangles.sort_unstable();
        triangles == [[0, 1, 2], [0, 2, 3]] || triangles == [[0, 1, 3], [1, 2, 3]]
    }
}

/// Axis-aligned bounds of a mesh in its local coordinate system.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshBounds3D {
    pub min: SemanticVec3,
    pub max: SemanticVec3,
}

fn mesh_bounds(points: impl IntoIterator<Item = SemanticVec3>) -> MeshBounds3D {
    let mut points = points.into_iter();
    let mut min = points.next().expect("validated nonempty mesh positions");
    let mut max = min;
    for point in points {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        min.z = min.z.min(point.z);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
        max.z = max.z.max(point.z);
    }
    MeshBounds3D { min, max }
}

/// Immutable indexed triangle data validated before it enters a resource arena.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshResource {
    positions: Vec<SemanticVec3>,
    normals: Option<Vec<SemanticVec3>>,
    has_usable_normals: bool,
    indices: Vec<u32>,
    bounds: MeshBounds3D,
    cairo_appearance: Option<Box<CairoSurfaceAppearance>>,
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
        // Shading admission needs this predicate frequently while objects move.
        // Compute it once with the immutable mesh payload instead of rescanning
        // every vertex normal for each changed pose.
        let has_usable_normals = normals.as_ref().is_some_and(|normals| {
            normals
                .iter()
                .all(|normal| normal.x.abs().max(normal.y.abs()).max(normal.z.abs()) > 0.0)
        });
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

        let bounds = mesh_bounds(positions.iter().copied());
        Ok(Self {
            positions,
            normals,
            has_usable_normals,
            indices,
            bounds,
            cairo_appearance: None,
        })
    }

    /// Attach immutable Cairo surface shading geometry after validating every
    /// retained endpoint and span. Ordinary meshes keep the unboxed `None`.
    pub fn with_cairo_appearance(
        mut self,
        appearance: CairoSurfaceAppearance,
    ) -> Result<Self, MeshResourceError> {
        if !appearance.is_finite() {
            return Err(MeshResourceError::NonFiniteCairoAppearance);
        }
        if appearance.boundary_controls.is_some()
            && !CairoSurfaceAppearance::has_canonical_quad_topology(
                self.positions.len(),
                &self.indices,
            )
        {
            return Err(MeshResourceError::InvalidCairoBoundaryTopology);
        }
        // Cubic paths stay inside the convex hull of their anchors and controls.
        // Retain that conservative extent once for world bounds and depth ordering.
        // Rebuild from anchors so replacing appearance can also shrink the bounds.
        self.bounds = mesh_bounds(
            self.positions
                .iter()
                .copied()
                .chain(appearance.boundary_controls.into_iter().flatten().flatten()),
        );
        self.cairo_appearance = Some(Box::new(appearance));
        Ok(self)
    }

    pub fn positions(&self) -> &[SemanticVec3] {
        &self.positions
    }
    pub fn normals(&self) -> Option<&[SemanticVec3]> {
        self.normals.as_deref()
    }
    /// Whether every authored vertex has a finite, non-zero normal. This
    /// immutable summary is safe to query in O(1) from renderer preparation.
    pub const fn has_usable_normals(&self) -> bool {
        self.has_usable_normals
    }
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }
    /// One retained triangle or two quad triangles sharing an edge. This is the common bounded
    /// face contract for Cairo appearance and object-depth translucency.
    pub fn is_single_face(&self) -> bool {
        match (self.positions.len(), self.indices.len()) {
            (3, 3) => {
                let mut indices = [self.indices[0], self.indices[1], self.indices[2]];
                indices.sort_unstable();
                indices == [0, 1, 2]
            }
            (4, 6) => {
                let mut first = [self.indices[0], self.indices[1], self.indices[2]];
                let mut second = [self.indices[3], self.indices[4], self.indices[5]];
                first.sort_unstable();
                second.sort_unstable();
                first.windows(2).all(|p| p[0] != p[1])
                    && second.windows(2).all(|p| p[0] != p[1])
                    && first.iter().filter(|i| second.contains(i)).count() == 2
            }
            _ => false,
        }
    }
    pub const fn bounds(&self) -> MeshBounds3D {
        self.bounds
    }
    pub fn cairo_appearance(&self) -> Option<&CairoSurfaceAppearance> {
        self.cairo_appearance.as_deref()
    }

    /// Logical payload bytes retained by the resource, excluding allocator overhead.
    pub fn retained_bytes(&self) -> usize {
        self.positions.len() * std::mem::size_of::<SemanticVec3>()
            + self.normals.as_ref().map_or(0, |values| {
                values.len() * std::mem::size_of::<SemanticVec3>()
            })
            + self.indices.len() * std::mem::size_of::<u32>()
            + self
                .cairo_appearance
                .as_ref()
                .map_or(0, |_| std::mem::size_of::<CairoSurfaceAppearance>())
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
    NonFiniteCairoAppearance,
    InvalidCairoBoundaryTopology,
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
            Self::NonFiniteCairoAppearance => {
                f.write_str("mesh has non-finite Cairo surface appearance data")
            }
            Self::InvalidCairoBoundaryTopology => {
                f.write_str("Cairo boundary controls require a canonical four-vertex quad")
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
        assert!(mesh.cairo_appearance().is_none());
    }

    #[test]
    fn cairo_appearance_is_optional_validated_retained_and_part_of_equality() {
        let appearance = CairoSurfaceAppearance {
            p0: SemanticVec3::ZERO,
            p6: SemanticVec3::new(1.0, 2.0 / 3.0, 1.333334444),
            span_p3_p0: SemanticVec3::new(1.0, 0.0, 1.0),
            span_p12_p0: SemanticVec3::new(0.0, 1.0, 1.0),
            span_p9_p6: SemanticVec3::new(-1.0 / 3.0, 1.0 / 3.0, 0.0),
            span_p3_p6: SemanticVec3::new(0.0, -2.0 / 3.0, -0.333334444),
            boundary_controls: None,
        };
        let plain = valid();
        let retained = plain.clone().with_cairo_appearance(appearance).unwrap();
        assert_eq!(retained.cairo_appearance(), Some(&appearance));
        assert_ne!(retained, plain);
        assert_eq!(
            retained.retained_bytes(),
            plain.retained_bytes() + std::mem::size_of::<CairoSurfaceAppearance>()
        );
        assert_eq!(
            plain.with_cairo_appearance(CairoSurfaceAppearance {
                p6: SemanticVec3::new(f64::NAN, 0.0, 0.0),
                ..appearance
            }),
            Err(MeshResourceError::NonFiniteCairoAppearance)
        );
    }

    #[test]
    fn cairo_boundary_controls_require_finite_points_and_canonical_quad_topology() {
        let appearance = CairoSurfaceAppearance {
            p0: SemanticVec3::ZERO,
            p6: SemanticVec3::ZERO,
            span_p3_p0: SemanticVec3::ZERO,
            span_p12_p0: SemanticVec3::ZERO,
            span_p9_p6: SemanticVec3::ZERO,
            span_p3_p6: SemanticVec3::ZERO,
            boundary_controls: Some([[SemanticVec3::ZERO; 2]; 4]),
        };
        let positions = vec![
            SemanticVec3::ZERO,
            SemanticVec3::new(1.0, 0.0, 0.0),
            SemanticVec3::new(1.0, 1.0, 0.0),
            SemanticVec3::new(0.0, 1.0, 0.0),
        ];
        let canonical = MeshResource::new(positions.clone(), None, vec![0, 1, 3, 1, 2, 3]).unwrap();
        let retained = canonical.clone().with_cairo_appearance(appearance).unwrap();
        assert_eq!(
            retained.cairo_appearance().unwrap().boundary_controls,
            appearance.boundary_controls
        );

        let alternate_diagonal =
            MeshResource::new(positions.clone(), None, vec![0, 1, 2, 0, 2, 3]).unwrap();
        assert!(alternate_diagonal.with_cairo_appearance(appearance).is_ok());

        let triangle = valid();
        assert_eq!(
            triangle.with_cairo_appearance(appearance),
            Err(MeshResourceError::InvalidCairoBoundaryTopology)
        );
        let noncanonical_quad = MeshResource::new(positions, None, vec![0, 1, 2, 0, 1, 3]).unwrap();
        assert_eq!(
            noncanonical_quad.with_cairo_appearance(appearance),
            Err(MeshResourceError::InvalidCairoBoundaryTopology)
        );

        let mut non_finite = appearance;
        non_finite.boundary_controls.as_mut().unwrap()[2][1].z = f64::INFINITY;
        assert_eq!(
            canonical.with_cairo_appearance(non_finite),
            Err(MeshResourceError::NonFiniteCairoAppearance)
        );
    }

    #[test]
    fn curved_perimeter_bounds_include_controls_and_shrink_when_replaced() {
        let plain = MeshResource::new(
            vec![
                SemanticVec3::ZERO,
                SemanticVec3::new(1.0, 0.0, 0.0),
                SemanticVec3::new(1.0, 1.0, 0.0),
                SemanticVec3::new(0.0, 1.0, 0.0),
            ],
            None,
            vec![0, 1, 2, 0, 2, 3],
        )
        .unwrap();
        let appearance = CairoSurfaceAppearance {
            p0: SemanticVec3::ZERO,
            p6: SemanticVec3::ZERO,
            span_p3_p0: SemanticVec3::ZERO,
            span_p12_p0: SemanticVec3::ZERO,
            span_p9_p6: SemanticVec3::ZERO,
            span_p3_p6: SemanticVec3::ZERO,
            boundary_controls: Some([
                [
                    SemanticVec3::new(-2.0, 0.0, 3.0),
                    SemanticVec3::new(4.0, 2.0, -1.0),
                ],
                [SemanticVec3::ZERO; 2],
                [SemanticVec3::ZERO; 2],
                [SemanticVec3::ZERO; 2],
            ]),
        };
        let curved = plain.clone().with_cairo_appearance(appearance).unwrap();
        assert_eq!(
            curved.bounds(),
            MeshBounds3D {
                min: SemanticVec3::new(-2.0, 0.0, -1.0),
                max: SemanticVec3::new(4.0, 2.0, 3.0),
            }
        );
        assert_eq!(curved.positions(), plain.positions());
        let straight = curved
            .with_cairo_appearance(CairoSurfaceAppearance {
                boundary_controls: None,
                ..appearance
            })
            .unwrap();
        assert_eq!(straight.bounds(), plain.bounds());
    }

    #[test]
    fn caches_normal_usability_for_constant_time_renderer_queries() {
        let count = 100_000;
        let positions = vec![SemanticVec3::ZERO; count];
        let normals = vec![SemanticVec3::new(0.0, 0.0, 1.0); count];
        let indices = vec![0, 0, 0];
        let usable = MeshResource::new(positions.clone(), Some(normals), indices.clone()).unwrap();
        assert!(usable.has_usable_normals());
        // Repeated access exercises only the cached scalar; it does not need
        // to inspect the immutable 100k-element normals array again.
        for _ in 0..1_000 {
            assert!(usable.has_usable_normals());
        }

        let mut normals = vec![SemanticVec3::new(0.0, 0.0, 1.0); count];
        normals[count / 2] = SemanticVec3::ZERO;
        let unusable = MeshResource::new(positions, Some(normals), indices).unwrap();
        assert!(!unusable.has_usable_normals());
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
