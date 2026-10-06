//! Bounded UV sampling and indexed topology for renderer-independent surfaces.

pub use noon_core::CairoSurfaceAppearance;
use noon_core::{MeshResource, MeshResourceError, SemanticVec3};

/// Hard limits applied before allocating callback-driven surface samples.
pub const MAX_SURFACE_CELLS: usize = 1_000_000;
pub const MAX_SURFACE_VERTICES: usize = MAX_SURFACE_CELLS + 1_000_002;
/// Each Cairo-style quad evaluates four cubic segments with four points each.
pub const MAX_CAIRO_SURFACE_SAMPLES: usize = MAX_SURFACE_CELLS * 16;
/// Manim v0.21 temporarily scales cubic handles by this amount before mapping.
pub const CAIRO_SURFACE_HANDLE_SCALE: f64 = 0.00001;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceError {
    InvalidRange,
    InvalidResolution,
    SurfaceTooLarge,
    NonFinitePosition,
    NonFiniteCairoControlPoint,
    NonFiniteNormal,
    MixedNormalAvailability,
    SampleCountMismatch { expected: usize, actual: usize },
    InconsistentSharedSample,
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
            Self::NonFiniteCairoControlPoint => {
                f.write_str("surface Cairo handle expansion produced a non-finite point")
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
            Self::InconsistentSharedSample => {
                f.write_str("surface callback returned different positions for a shared UV")
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

    /// Iterate Manim v0.21 Cairo control-point sample inputs. Each quad emits
    /// sixteen inputs in closed-path order, including repeated anchors. A
    /// deterministic callback must return the same position for every repeat.
    pub fn cairo_coordinates(self) -> CairoSurfaceCoordinates {
        CairoSurfaceCoordinates {
            plan: self,
            next: 0,
            sample_count: self
                .cell_count
                .checked_mul(16)
                .expect("bounded surface cell count fits Cairo sample count"),
        }
    }

    /// Sample the optional Cairo control-point path and retain its endpoint
    /// shading data beside the ordinary four-corner cells.
    pub fn sample_cairo<F>(self, mut callback: F) -> Result<CairoSurfaceGrid, SurfaceError>
    where
        F: FnMut(f64, f64) -> SemanticVec3,
    {
        self.finish_cairo_samples(self.cairo_coordinates().map(|(u, v)| callback(u, v)))
    }

    /// Finish externally sampled Cairo control points. At most the expected
    /// sample count plus one item is consumed, so malformed or unbounded input
    /// cannot cause an unbounded read or semantic publication.
    pub fn finish_cairo_samples<I>(self, samples: I) -> Result<CairoSurfaceGrid, SurfaceError>
    where
        I: IntoIterator<Item = SemanticVec3>,
    {
        let expected = self
            .cell_count
            .checked_mul(16)
            .filter(|count| *count <= MAX_CAIRO_SURFACE_SAMPLES)
            .ok_or(SurfaceError::SurfaceTooLarge)?;
        let mut samples = samples.into_iter();
        let mut positions = vec![None; self.vertex_count];
        let mut appearances = Vec::with_capacity(self.cell_count);
        let [_, v_cells] = self.resolution;
        let stride = v_cells + 1;
        let mut actual = 0;
        for u_cell in 0..self.resolution[0] {
            for v_cell in 0..v_cells {
                let mut points = [SemanticVec3::ZERO; 16];
                for point in &mut points {
                    let Some(position) = samples.next() else {
                        return Err(SurfaceError::SampleCountMismatch { expected, actual });
                    };
                    actual += 1;
                    if !position.is_finite() {
                        return Err(SurfaceError::NonFinitePosition);
                    }
                    *point = position;
                }
                for (first, repeated) in [(0, 15), (3, 4), (7, 8), (11, 12)] {
                    if points[first] != points[repeated] {
                        return Err(SurfaceError::InconsistentSharedSample);
                    }
                }
                let expanded_handles = [
                    expand_cairo_handle(points[0], points[1]),
                    expand_cairo_handle(points[3], points[2]),
                    expand_cairo_handle(points[4], points[5]),
                    expand_cairo_handle(points[7], points[6]),
                    expand_cairo_handle(points[8], points[9]),
                    expand_cairo_handle(points[11], points[10]),
                    expand_cairo_handle(points[12], points[13]),
                    expand_cairo_handle(points[15], points[14]),
                ];
                if expanded_handles.iter().any(|point| !point.is_finite()) {
                    return Err(SurfaceError::NonFiniteCairoControlPoint);
                }
                let a = u_cell * stride + v_cell;
                let b = (u_cell + 1) * stride + v_cell;
                let c = b + 1;
                let d = a + 1;
                for (index, position) in [
                    (a, points[0]),
                    (b, points[3]),
                    (c, points[7]),
                    (d, points[11]),
                ] {
                    if let Some(previous) = positions[index] {
                        if previous != position {
                            return Err(SurfaceError::InconsistentSharedSample);
                        }
                    } else {
                        positions[index] = Some(position);
                    }
                }
                let p0 = points[0];
                let p6 = expanded_handles[3];
                let p9 = expanded_handles[4];
                let appearance = CairoSurfaceAppearance {
                    p0,
                    p6,
                    span_p3_p0: subtract(points[3], p0),
                    span_p12_p0: subtract(points[12], p0),
                    span_p9_p6: subtract(p9, p6),
                    span_p3_p6: subtract(points[3], p6),
                };
                if !appearance.is_finite() {
                    return Err(SurfaceError::NonFiniteCairoControlPoint);
                }
                appearances.push(appearance);
            }
        }
        if samples.next().is_some() {
            return Err(SurfaceError::SampleCountMismatch {
                expected,
                actual: expected + 1,
            });
        }
        let positions = positions
            .into_iter()
            .map(|position| position.expect("all grid vertices belong to a cell"))
            .collect::<Vec<_>>();
        let indices = grid_indices(self.resolution)?;
        let normals = derive_normals_allow_degenerate(&positions, &indices)?;
        let grid = SurfaceGrid {
            plan: self,
            positions,
            normals,
            indices,
        };
        Ok(CairoSurfaceGrid { grid, appearances })
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
pub struct CairoSurfaceCoordinates {
    plan: UvSurfacePlan,
    next: usize,
    sample_count: usize,
}

impl Iterator for CairoSurfaceCoordinates {
    type Item = (f64, f64);

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.sample_count {
            return None;
        }
        let linear_cell = self.next / 16;
        let point_index = self.next % 16;
        let [u_cells, v_cells] = self.plan.resolution;
        let u_cell = linear_cell / v_cells;
        let v_cell = linear_cell % v_cells;
        let u0 = lerp(self.plan.u_range, u_cell, u_cells);
        let u1 = lerp(self.plan.u_range, u_cell + 1, u_cells);
        let v0 = lerp(self.plan.v_range, v_cell, v_cells);
        let v1 = lerp(self.plan.v_range, v_cell + 1, v_cells);
        let uv = match point_index {
            0 | 15 => (u0, v0),
            1 => (cairo_handle_input(u0, u1, 1.0 / 3.0, false), v0),
            2 => (cairo_handle_input(u0, u1, 2.0 / 3.0, true), v0),
            3 | 4 => (u1, v0),
            5 => (u1, cairo_handle_input(v0, v1, 1.0 / 3.0, false)),
            6 => (u1, cairo_handle_input(v0, v1, 2.0 / 3.0, true)),
            7 | 8 => (u1, v1),
            9 => (cairo_handle_input(u1, u0, 1.0 / 3.0, false), v1),
            10 => (cairo_handle_input(u1, u0, 2.0 / 3.0, true), v1),
            11 | 12 => (u0, v1),
            13 => (u0, cairo_handle_input(v1, v0, 1.0 / 3.0, false)),
            14 => (u0, cairo_handle_input(v1, v0, 2.0 / 3.0, true)),
            _ => unreachable!(),
        };
        self.next += 1;
        Some(uv)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.sample_count - self.next;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for CairoSurfaceCoordinates {}

#[derive(Clone, Debug, PartialEq)]
pub struct CairoSurfaceGrid {
    grid: SurfaceGrid,
    appearances: Vec<CairoSurfaceAppearance>,
}

fn expand_cairo_handle(anchor: SemanticVec3, handle: SemanticVec3) -> SemanticVec3 {
    let factor = 1.0 / CAIRO_SURFACE_HANDLE_SCALE;
    SemanticVec3::new(
        anchor.x + factor * (handle.x - anchor.x),
        anchor.y + factor * (handle.y - anchor.y),
        anchor.z + factor * (handle.z - anchor.z),
    )
}

impl CairoSurfaceGrid {
    pub fn grid(&self) -> &SurfaceGrid {
        &self.grid
    }

    pub fn appearances(&self) -> &[CairoSurfaceAppearance] {
        &self.appearances
    }

    /// Pair appearance endpoints with the existing lazily extracted quad mesh.
    pub fn cells(&self) -> impl Iterator<Item = (SurfaceCell, CairoSurfaceAppearance)> + '_ {
        self.grid.cells().zip(self.appearances.iter().copied())
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
    derive_normals_with_policy(positions, indices, false)
}

fn derive_normals_allow_degenerate(
    positions: &[SemanticVec3],
    indices: &[u32],
) -> Result<Vec<SemanticVec3>, SurfaceError> {
    derive_normals_with_policy(positions, indices, true)
}

fn derive_normals_with_policy(
    positions: &[SemanticVec3],
    indices: &[u32],
    allow_degenerate_vertices: bool,
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
    let mut has_usable_normal = false;
    let normals = sums
        .into_iter()
        .map(|sum| match normalize(sum) {
            Some(normal) => {
                has_usable_normal = true;
                Ok(normal)
            }
            None if allow_degenerate_vertices => Ok(SemanticVec3::ZERO),
            None => Err(SurfaceError::DegenerateNormal),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if allow_degenerate_vertices && !has_usable_normal {
        return Err(SurfaceError::DegenerateNormal);
    }
    Ok(normals)
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

fn cairo_handle_input(start: f64, end: f64, handle_ratio: f64, at_end: bool) -> f64 {
    let handle = start + (end - start) * handle_ratio;
    let anchor = if at_end { end } else { start };
    anchor + CAIRO_SURFACE_HANDLE_SCALE * (handle - anchor)
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
    fn cairo_sampling_matches_manims_closed_quad_handle_contract() {
        let plan = UvSurfacePlan::new([0.0, 1.0], [0.0, 1.0], [1, 1]).unwrap();
        let coordinates = plan.cairo_coordinates().collect::<Vec<_>>();
        let near_zero = CAIRO_SURFACE_HANDLE_SCALE * (1.0 / 3.0);
        let near_one = 1.0 + CAIRO_SURFACE_HANDLE_SCALE * ((2.0 / 3.0) - 1.0);
        let reverse_end_handle = 1.0 + (0.0 - 1.0) * (2.0 / 3.0);
        let reverse_near_zero = 0.0 + CAIRO_SURFACE_HANDLE_SCALE * (reverse_end_handle - 0.0);
        assert_eq!(coordinates.len(), 16);
        assert_eq!(
            coordinates,
            vec![
                (0.0, 0.0),
                (near_zero, 0.0),
                (near_one, 0.0),
                (1.0, 0.0),
                (1.0, 0.0),
                (1.0, near_zero),
                (1.0, near_one),
                (1.0, 1.0),
                (1.0, 1.0),
                (near_one, 1.0),
                (reverse_near_zero, 1.0),
                (0.0, 1.0),
                (0.0, 1.0),
                (0.0, near_one),
                (0.0, reverse_near_zero),
                (0.0, 0.0),
            ]
        );

        let mut callback_coordinates = Vec::new();
        let sampled = plan
            .sample_cairo(|u, v| {
                callback_coordinates.push((u, v));
                SemanticVec3::new(u, v, u * u + v * v)
            })
            .unwrap();
        assert_eq!(callback_coordinates, coordinates);
        assert_eq!(sampled.appearances().len(), 1);
        assert_eq!(sampled.grid().cells().count(), 1);
        assert_eq!(
            sampled.grid().cells().next().unwrap().positions,
            [
                SemanticVec3::new(0.0, 0.0, 0.0),
                SemanticVec3::new(1.0, 0.0, 1.0),
                SemanticVec3::new(1.0, 1.0, 2.0),
                SemanticVec3::new(0.0, 1.0, 1.0),
            ]
        );
        let appearance = sampled.appearances()[0];
        assert_eq!(appearance.p0, SemanticVec3::ZERO);
        assert!((appearance.p6.x - 1.0).abs() < 1e-12);
        assert!((appearance.p6.y - (2.0 / 3.0)).abs() < 1e-9);
        assert!((appearance.p6.z - 1.333334444).abs() < 1e-9);
        assert_eq!(sampled.cells().next().unwrap().0.uv_cell, [0, 0]);
    }

    #[test]
    fn cairo_sampling_accepts_flat_faces_and_rejects_degenerate_meshes() {
        let plan = UvSurfacePlan::new([0.0, 1.0], [0.0, 1.0], [1, 1]).unwrap();
        let flat = plan
            .sample_cairo(|u, v| SemanticVec3::new(u, v, 0.0))
            .unwrap();
        assert_eq!(flat.appearances()[0].p0, SemanticVec3::ZERO);
        assert_eq!(
            flat.appearances()[0].span_p3_p0,
            SemanticVec3::new(1.0, 0.0, 0.0)
        );
        assert_eq!(
            flat.appearances()[0].span_p12_p0,
            SemanticVec3::new(0.0, 1.0, 0.0)
        );
        let flat_span = flat.cells().next().unwrap().1.span_p3_p6;
        assert!(flat_span.x.abs() < 1e-12);
        assert!((flat_span.y + 2.0 / 3.0).abs() < 1e-9);
        assert!(flat_span.z.abs() < 1e-12);

        assert_eq!(
            plan.sample_cairo(|u, _| SemanticVec3::new(u, 0.0, 0.0)),
            Err(SurfaceError::DegenerateNormal)
        );
        assert_eq!(
            plan.sample(|u, _| SurfaceSample::position(SemanticVec3::new(u, 0.0, 0.0))),
            Err(SurfaceError::DegenerateNormal),
            "ordinary Surface sampling retains its strict vertex-normal contract"
        );
    }

    #[test]
    fn cairo_sampling_accepts_spherical_poles_and_retains_fallback_spans() {
        let plan = UvSurfacePlan::new(
            [0.0, std::f64::consts::TAU],
            [0.0, std::f64::consts::PI],
            [8, 6],
        )
        .unwrap();
        let sphere = |u: f64, v: f64| {
            let (sin_v, cos_v) = v.sin_cos();
            let (sin_u, cos_u) = u.sin_cos();
            SemanticVec3::new(cos_u * sin_v, sin_u * sin_v, -cos_v)
        };
        let grid = plan.sample_cairo(sphere).unwrap();
        assert_eq!(
            plan.sample(|u, v| SurfaceSample::position(sphere(u, v))),
            Err(SurfaceError::DegenerateNormal),
            "Cairo's span-derived material does not relax ordinary mesh normals"
        );

        assert_eq!(grid.grid().positions().len(), 9 * 7);
        assert_eq!(grid.grid().indices().len(), 8 * 6 * 6);
        assert!(grid.grid().normals().contains(&SemanticVec3::ZERO));
        assert!(grid
            .grid()
            .normals()
            .iter()
            .any(|normal| *normal != SemanticVec3::ZERO));
        assert_eq!(grid.appearances().len(), 8 * 6);
        assert_eq!(grid.appearances()[0].span_p3_p0, SemanticVec3::ZERO);
        assert_ne!(grid.appearances()[0].span_p12_p0, SemanticVec3::ZERO);
        assert!(grid
            .appearances()
            .iter()
            .all(CairoSurfaceAppearance::is_finite));
        assert!(grid.appearances().iter().any(|appearance| {
            appearance.span_p3_p0 != SemanticVec3::ZERO
                || appearance.span_p12_p0 != SemanticVec3::ZERO
                || appearance.span_p9_p6 != SemanticVec3::ZERO
                || appearance.span_p3_p6 != SemanticVec3::ZERO
        }));
    }

    #[test]
    fn cairo_external_samples_are_bounded_finite_and_coherent_at_shared_uvs() {
        let plan = UvSurfacePlan::new([0.0, 1.0], [0.0, 1.0], [1, 1]).unwrap();
        assert_eq!(
            plan.finish_cairo_samples(std::iter::repeat_n(SemanticVec3::ZERO, 15)),
            Err(SurfaceError::SampleCountMismatch {
                expected: 16,
                actual: 15,
            })
        );
        let mut consumed = 0;
        assert_eq!(
            plan.finish_cairo_samples(std::iter::repeat_with(|| {
                consumed += 1;
                SemanticVec3::ZERO
            })),
            Err(SurfaceError::SampleCountMismatch {
                expected: 16,
                actual: 17,
            })
        );
        assert_eq!(consumed, 17);
        let mut non_finite = vec![SemanticVec3::ZERO; 16];
        non_finite[9].x = f64::NAN;
        assert_eq!(
            plan.finish_cairo_samples(non_finite),
            Err(SurfaceError::NonFinitePosition)
        );
        let mut overflowed_handle = vec![SemanticVec3::ZERO; 16];
        overflowed_handle[6].z = f64::MAX;
        assert_eq!(
            plan.finish_cairo_samples(overflowed_handle),
            Err(SurfaceError::NonFiniteCairoControlPoint)
        );
        let mut inconsistent = plan
            .cairo_coordinates()
            .map(|(u, v)| SemanticVec3::new(u, v, 0.0))
            .collect::<Vec<_>>();
        inconsistent[4].z = 1.0; // repeated B anchor
        assert_eq!(
            plan.finish_cairo_samples(inconsistent),
            Err(SurfaceError::InconsistentSharedSample)
        );

        let adjacent = UvSurfacePlan::new([0.0, 1.0], [0.0, 1.0], [2, 1]).unwrap();
        let mut inconsistent = adjacent
            .cairo_coordinates()
            .map(|(u, v)| SemanticVec3::new(u, v, 0.0))
            .collect::<Vec<_>>();
        inconsistent[16].z = 1.0; // first anchor of the adjacent UV cell
        inconsistent[31].z = 1.0; // its repeated closed-path anchor
        assert_eq!(
            adjacent.finish_cairo_samples(inconsistent),
            Err(SurfaceError::InconsistentSharedSample)
        );
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
