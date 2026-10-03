//! Bounded UV sampling and indexed topology for renderer-independent surfaces.

use noon_core::{MeshResource, MeshResourceError, SemanticVec3};

/// Hard limits applied before allocating callback-driven surface samples.
pub const MAX_SURFACE_CELLS: usize = 1_000_000;
pub const MAX_SURFACE_VERTICES: usize = MAX_SURFACE_CELLS + 1_000_002;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceError {
    InvalidRange,
    InvalidResolution,
    SurfaceTooLarge,
    NonFinitePosition,
    NonFiniteNormal,
    MixedNormalAvailability,
    SampleCountMismatch { expected: usize, actual: usize },
    DegenerateNormal,
    Mesh(MeshResourceError),
}

impl std::fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRange => {
                f.write_str("surface parameter ranges must be finite and increasing")
            }
            Self::InvalidResolution => {
                f.write_str("surface resolution must contain positive cell counts")
            }
            Self::SurfaceTooLarge => f.write_str("surface exceeds the bounded mesh size"),
            Self::NonFinitePosition => {
                f.write_str("surface callback returned a non-finite position")
            }
            Self::NonFiniteNormal => f.write_str("surface callback returned a non-finite normal"),
            Self::MixedNormalAvailability => {
                f.write_str("surface callback must provide either all normals or none")
            }
            Self::SampleCountMismatch { expected, actual } => {
                write!(
                    f,
                    "surface expected {expected} samples but received {actual}"
                )
            }
            Self::DegenerateNormal => {
                f.write_str("surface topology cannot produce a finite non-zero normal")
            }
            Self::Mesh(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for SurfaceError {}

impl From<MeshResourceError> for SurfaceError {
    fn from(value: MeshResourceError) -> Self {
        Self::Mesh(value)
    }
}

/// One callback result at a sampled UV coordinate. Normals may be omitted as a
/// group, in which case smooth area-weighted normals are derived from topology.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceSample {
    pub position: SemanticVec3,
    pub normal: Option<SemanticVec3>,
}

impl SurfaceSample {
    pub const fn position(position: SemanticVec3) -> Self {
        Self {
            position,
            normal: None,
        }
    }

    pub const fn with_normal(position: SemanticVec3, normal: SemanticVec3) -> Self {
        Self {
            position,
            normal: Some(normal),
        }
    }
}

/// A deterministic rectangular sampling domain. Resolution counts cells, not
/// samples; both endpoint rows are retained, including periodic seams.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UvSurfacePlan {
    u_range: [f64; 2],
    v_range: [f64; 2],
    resolution: [usize; 2],
    vertex_count: usize,
    cell_count: usize,
}

impl UvSurfacePlan {
    pub fn new(
        u_range: [f64; 2],
        v_range: [f64; 2],
        resolution: [usize; 2],
    ) -> Result<Self, SurfaceError> {
        if u_range
            .iter()
            .chain(v_range.iter())
            .any(|value| !value.is_finite())
            || u_range[0] >= u_range[1]
            || v_range[0] >= v_range[1]
            || !(u_range[1] - u_range[0]).is_finite()
            || !(v_range[1] - v_range[0]).is_finite()
        {
            return Err(SurfaceError::InvalidRange);
        }
        if resolution.contains(&0) {
            return Err(SurfaceError::InvalidResolution);
        }
        let cell_count = resolution[0]
            .checked_mul(resolution[1])
            .filter(|&count| count <= MAX_SURFACE_CELLS)
            .ok_or(SurfaceError::SurfaceTooLarge)?;
        let vertex_count = resolution[0]
            .checked_add(1)
            .and_then(|u| resolution[1].checked_add(1).and_then(|v| u.checked_mul(v)))
            .filter(|&count| count <= MAX_SURFACE_VERTICES)
            .ok_or(SurfaceError::SurfaceTooLarge)?;
        Ok(Self {
            u_range,
            v_range,
            resolution,
            vertex_count,
            cell_count,
        })
    }

    pub const fn resolution(self) -> [usize; 2] {
        self.resolution
    }
    pub const fn vertex_count(self) -> usize {
        self.vertex_count
    }
    pub const fn cell_count(self) -> usize {
        self.cell_count
    }

    /// Iterate the exact bounded coordinates used by callback sampling.
    pub fn coordinates(self) -> SurfaceCoordinates {
        SurfaceCoordinates {
            plan: self,
            next: 0,
        }
    }

    pub fn sample<F>(self, mut callback: F) -> Result<SurfaceGrid, SurfaceError>
    where
        F: FnMut(f64, f64) -> SurfaceSample,
    {
        self.finish_samples(self.coordinates().map(|(u, v)| callback(u, v)))
    }

    /// Validate an externally sampled payload in the same coordinate order as
    /// `sample`. The iterator is consumed at most `vertex_count + 1` elements.
    pub fn finish_samples<I>(self, samples: I) -> Result<SurfaceGrid, SurfaceError>
    where
        I: IntoIterator<Item = SurfaceSample>,
    {
        let mut positions = Vec::with_capacity(self.vertex_count);
        let mut supplied_normals = Vec::with_capacity(self.vertex_count);
        let mut normals_present = None;
        let mut actual = 0;
        for sample in samples
            .into_iter()
            .take(self.vertex_count.saturating_add(1))
        {
            actual += 1;
            if actual > self.vertex_count {
                return Err(SurfaceError::SampleCountMismatch {
                    expected: self.vertex_count,
                    actual,
                });
            }
            if !sample.position.is_finite() {
                return Err(SurfaceError::NonFinitePosition);
            }
            let has_normal = sample.normal.is_some();
            if normals_present.is_some_and(|previous| previous != has_normal) {
                return Err(SurfaceError::MixedNormalAvailability);
            }
            normals_present = Some(has_normal);
            if let Some(normal) = sample.normal {
                if !normal.is_finite() {
                    return Err(SurfaceError::NonFiniteNormal);
                }
                supplied_normals.push(normalize(normal).ok_or(SurfaceError::DegenerateNormal)?);
            }
            positions.push(sample.position);
        }
        if actual != self.vertex_count {
            return Err(SurfaceError::SampleCountMismatch {
                expected: self.vertex_count,
                actual,
            });
        }
        let indices = grid_indices(self.resolution)?;
        let normals = if normals_present == Some(true) {
            supplied_normals
        } else {
            derive_normals(&positions, &indices)?
        };
        Ok(SurfaceGrid {
            plan: self,
            positions,
            normals,
            indices,
        })
    }
}

#[derive(Clone, Debug)]
pub struct SurfaceCoordinates {
    plan: UvSurfacePlan,
    next: usize,
}

impl Iterator for SurfaceCoordinates {
    type Item = (f64, f64);

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.plan.vertex_count {
            return None;
        }
        let [u_cells, v_cells] = self.plan.resolution;
        let stride = v_cells + 1;
        let u_index = self.next / stride;
        let v_index = self.next % stride;
        self.next += 1;
        Some((
            lerp(self.plan.u_range, u_index, u_cells),
            lerp(self.plan.v_range, v_index, v_cells),
        ))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.plan.vertex_count - self.next;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for SurfaceCoordinates {}

/// A fully sampled surface. Its cells can be emitted independently for family
/// semantics, or the complete grid can be retained as one indexed mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceGrid {
    plan: UvSurfacePlan,
    positions: Vec<SemanticVec3>,
    normals: Vec<SemanticVec3>,
    indices: Vec<u32>,
}

impl SurfaceGrid {
    pub const fn plan(&self) -> UvSurfacePlan {
        self.plan
    }
    pub fn positions(&self) -> &[SemanticVec3] {
        &self.positions
    }
    pub fn normals(&self) -> &[SemanticVec3] {
        &self.normals
    }

    pub fn to_mesh_resource(&self) -> Result<MeshResource, SurfaceError> {
        MeshResource::new(
            self.positions.clone(),
            Some(self.normals.clone()),
            self.indices.clone(),
        )
        .map_err(Into::into)
    }

    pub fn into_mesh_resource(self) -> Result<MeshResource, SurfaceError> {
        MeshResource::new(self.positions, Some(self.normals), self.indices).map_err(Into::into)
    }

    /// Iterate in Cairo-compatible u-outer/v-inner order.
    pub fn cells(&self) -> impl Iterator<Item = SurfaceCell> + '_ {
        let [_, v_cells] = self.plan.resolution;
        (0..self.plan.cell_count).map(move |linear| {
            let u = linear / v_cells;
            let v = linear % v_cells;
            let stride = v_cells + 1;
            let a = u * stride + v;
            let b = (u + 1) * stride + v;
            let c = b + 1;
            let d = a + 1;
            SurfaceCell {
                uv_cell: [u, v],
                positions: [
                    self.positions[a],
                    self.positions[b],
                    self.positions[c],
                    self.positions[d],
                ],
                normals: [
                    self.normals[a],
                    self.normals[b],
                    self.normals[c],
                    self.normals[d],
                ],
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceCell {
    pub uv_cell: [usize; 2],
    /// Corner order is (u-low,v-low), (u-high,v-low), (u-high,v-high), (u-low,v-high).
    pub positions: [SemanticVec3; 4],
    pub normals: [SemanticVec3; 4],
}

impl SurfaceCell {
    /// Retain this sampled quad as one independently addressable triangle mesh.
    /// Vertex ordering is the documented low-low, high-low, high-high, low-high.
    pub fn into_mesh_resource(self) -> Result<MeshResource, SurfaceError> {
        MeshResource::new(
            self.positions.to_vec(),
            Some(self.normals.to_vec()),
            vec![0, 1, 3, 1, 2, 3],
        )
        .map_err(Into::into)
    }
}

pub(crate) fn grid_indices(resolution: [usize; 2]) -> Result<Vec<u32>, SurfaceError> {
    let [u_cells, v_cells] = resolution;
    let count = u_cells
        .checked_mul(v_cells)
        .and_then(|cells| cells.checked_mul(6))
        .filter(|&count| count <= MAX_SURFACE_CELLS * 6)
        .ok_or(SurfaceError::SurfaceTooLarge)?;
    let stride = v_cells + 1;
    let mut indices = Vec::with_capacity(count);
    for u in 0..u_cells {
        for v in 0..v_cells {
            let a = u * stride + v;
            let b = (u + 1) * stride + v;
            let c = b + 1;
            let d = a + 1;
            indices.extend([a, b, d, b, c, d].map(|value| value as u32));
        }
    }
    Ok(indices)
}

pub(crate) fn normalize(value: SemanticVec3) -> Option<SemanticVec3> {
    if !value.is_finite() {
        return None;
    }
    let scale = value.x.abs().max(value.y.abs()).max(value.z.abs());
    if scale == 0.0 {
        return None;
    }
    let x = value.x / scale;
    let y = value.y / scale;
    let z = value.z / scale;
    let length = (x * x + y * y + z * z).sqrt();
    Some(SemanticVec3::new(x / length, y / length, z / length))
}

pub(crate) fn cross(a: SemanticVec3, b: SemanticVec3) -> SemanticVec3 {
    SemanticVec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

pub(crate) fn subtract(a: SemanticVec3, b: SemanticVec3) -> SemanticVec3 {
    SemanticVec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

pub(crate) fn derive_normals(
    positions: &[SemanticVec3],
    indices: &[u32],
) -> Result<Vec<SemanticVec3>, SurfaceError> {
    let mut sums = vec![SemanticVec3::ZERO; positions.len()];
    for triangle in indices.as_chunks::<3>().0 {
        let a = positions[triangle[0] as usize];
        let b = positions[triangle[1] as usize];
        let c = positions[triangle[2] as usize];
        let normal = cross(subtract(b, a), subtract(c, a));
        if !normal.is_finite() {
            return Err(SurfaceError::NonFiniteNormal);
        }
        for &index in triangle {
            let sum = &mut sums[index as usize];
            sum.x += normal.x;
            sum.y += normal.y;
            sum.z += normal.z;
            if !sum.is_finite() {
                return Err(SurfaceError::NonFiniteNormal);
            }
        }
    }
    sums.into_iter()
        .map(|sum| normalize(sum).ok_or(SurfaceError::DegenerateNormal))
        .collect()
}

fn lerp(range: [f64; 2], index: usize, steps: usize) -> f64 {
    if index == 0 {
        range[0]
    } else if index == steps {
        range[1]
    } else {
        range[0] + (range[1] - range[0]) * index as f64 / steps as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_counts_cells_and_preserves_uv_order() {
        let plan = UvSurfacePlan::new([0.0, 2.0], [0.0, 3.0], [2, 3]).unwrap();
        assert_eq!(plan.vertex_count(), 12);
        assert_eq!(plan.cell_count(), 6);
        assert_eq!(plan.coordinates().len(), 12);
        assert_eq!(
            plan.coordinates().take(4).collect::<Vec<_>>(),
            vec![(0.0, 0.0), (0.0, 1.0), (0.0, 2.0), (0.0, 3.0),]
        );
        let grid = plan
            .sample(|u, v| SurfaceSample::position(SemanticVec3::new(u, v, 0.0)))
            .unwrap();
        let external = plan
            .finish_samples(
                plan.coordinates()
                    .map(|(u, v)| SurfaceSample::position(SemanticVec3::new(u, v, 0.0))),
            )
            .unwrap();
        assert_eq!(
            external, grid,
            "both sampling paths share one implementation"
        );
        assert_eq!(
            grid.cells().map(|cell| cell.uv_cell).collect::<Vec<_>>(),
            vec![[0, 0], [0, 1], [0, 2], [1, 0], [1, 1], [1, 2]]
        );
        assert_eq!(grid.positions()[0], SemanticVec3::ZERO);
        assert_eq!(grid.positions()[3], SemanticVec3::new(0.0, 3.0, 0.0));
        let mesh = grid.to_mesh_resource().unwrap();
        assert_eq!(mesh.indices().len(), 36);
        assert_eq!(mesh.bounds().max, SemanticVec3::new(2.0, 3.0, 0.0));
        let cell = grid.cells().next().unwrap().into_mesh_resource().unwrap();
        assert_eq!(
            cell.positions(),
            &[
                SemanticVec3::ZERO,
                SemanticVec3::new(1.0, 0.0, 0.0),
                SemanticVec3::new(1.0, 1.0, 0.0),
                SemanticVec3::new(0.0, 1.0, 0.0)
            ]
        );
        assert_eq!(cell.indices(), &[0, 1, 3, 1, 2, 3]);
    }

    #[test]
    fn rejects_invalid_domains_callbacks_and_normal_contracts() {
        assert_eq!(
            UvSurfacePlan::new([0.0, f64::INFINITY], [0.0, 1.0], [1, 1]),
            Err(SurfaceError::InvalidRange)
        );
        assert_eq!(
            UvSurfacePlan::new([0.0, 1.0], [0.0, 1.0], [0, 1]),
            Err(SurfaceError::InvalidResolution)
        );
        assert_eq!(
            UvSurfacePlan::new([0.0, 1.0], [0.0, 1.0], [MAX_SURFACE_CELLS, 2]),
            Err(SurfaceError::SurfaceTooLarge)
        );
        let plan = UvSurfacePlan::new([0.0, 1.0], [0.0, 1.0], [1, 1]).unwrap();
        assert_eq!(
            plan.sample(|_, _| SurfaceSample::position(SemanticVec3::new(f64::NAN, 0.0, 0.0))),
            Err(SurfaceError::NonFinitePosition)
        );
        assert_eq!(
            plan.sample(|u, v| SurfaceSample {
                position: SemanticVec3::new(u, v, 0.0),
                normal: if u == 0.0 {
                    Some(SemanticVec3::new(0.0, 0.0, 1.0))
                } else {
                    None
                }
            }),
            Err(SurfaceError::MixedNormalAvailability)
        );
        assert_eq!(
            plan.sample(|u, v| SurfaceSample::with_normal(
                SemanticVec3::new(u, v, 0.0),
                SemanticVec3::ZERO
            )),
            Err(SurfaceError::DegenerateNormal)
        );
        let mut consumed = 0;
        assert_eq!(
            plan.finish_samples(std::iter::repeat_with(|| {
                consumed += 1;
                SurfaceSample::position(SemanticVec3::ZERO)
            })),
            Err(SurfaceError::SampleCountMismatch {
                expected: 4,
                actual: 5
            })
        );
        assert_eq!(consumed, 5);
        assert_eq!(
            plan.finish_samples([SurfaceSample::position(SemanticVec3::ZERO)]),
            Err(SurfaceError::SampleCountMismatch {
                expected: 4,
                actual: 1
            })
        );
    }
}
