//! Generated indexed triangle meshes for common closed solids.

use noon_core::{MeshResource, SemanticVec3};

use crate::surface::{
    normalize, SurfaceError, SurfaceSample, UvSurfacePlan, MAX_SURFACE_CELLS, MAX_SURFACE_VERTICES,
};

/// Build a shared indexed mesh from a bounded UV plan.
pub fn surface_mesh<F>(plan: UvSurfacePlan, callback: F) -> Result<MeshResource, SurfaceError>
where
    F: FnMut(f64, f64) -> SurfaceSample,
{
    plan.sample(callback)?.into_mesh_resource()
}

pub fn sphere_mesh(radius: f64, resolution: [usize; 2]) -> Result<MeshResource, SurfaceError> {
    sphere_mesh_range(
        radius,
        resolution,
        [0.0, std::f64::consts::TAU],
        [0.0, std::f64::consts::PI],
    )
}

pub fn sphere_mesh_range(
    radius: f64,
    resolution: [usize; 2],
    u_range: [f64; 2],
    v_range: [f64; 2],
) -> Result<MeshResource, SurfaceError> {
    positive(radius)?;
    if u_range[0] < 0.0
        || u_range[1] > std::f64::consts::TAU
        || v_range[0] < 0.0
        || v_range[1] > std::f64::consts::PI
    {
        return Err(SurfaceError::InvalidRange);
    }
    let plan = UvSurfacePlan::new(u_range, v_range, resolution)?;
    plan.sample(|u, v| {
        // Match Manim v0.21 Sphere.func: u is azimuth and v is polar angle.
        let (sin_v, cos_v) = v.sin_cos();
        let (sin_u, cos_u) = u.sin_cos();
        let normal = SemanticVec3::new(cos_u * sin_v, sin_u * sin_v, -cos_v);
        SurfaceSample::with_normal(scale(normal, radius), normal)
    })?
    .into_mesh_resource()
}

pub fn torus_mesh(
    major_radius: f64,
    minor_radius: f64,
    resolution: [usize; 2],
) -> Result<MeshResource, SurfaceError> {
    positive(major_radius)?;
    positive(minor_radius)?;
    if minor_radius >= major_radius {
        return Err(SurfaceError::InvalidRange);
    }
    let plan = UvSurfacePlan::new(
        [0.0, std::f64::consts::TAU],
        [0.0, std::f64::consts::TAU],
        resolution,
    )?;
    plan.sample(|u, v| {
        let (sin_u, cos_u) = u.sin_cos();
        let (sin_v, cos_v) = v.sin_cos();
        let radial = major_radius + minor_radius * cos_v;
        let position = SemanticVec3::new(radial * cos_u, radial * sin_u, minor_radius * sin_v);
        let normal = SemanticVec3::new(cos_v * cos_u, cos_v * sin_u, sin_v);
        SurfaceSample::with_normal(position, normal)
    })?
    .into_mesh_resource()
}

pub fn cylinder_mesh(
    radius: f64,
    height: f64,
    segments: usize,
) -> Result<MeshResource, SurfaceError> {
    cylinder_mesh_range(radius, height, segments, true, [0.0, std::f64::consts::TAU])
}

/// Build a cylinder whose side uses the requested azimuth interval. End disks
/// retain Manim's full-circle `show_ends` behavior for partial side sweeps.
pub fn cylinder_mesh_range(
    radius: f64,
    height: f64,
    segments: usize,
    show_ends: bool,
    angle_range: [f64; 2],
) -> Result<MeshResource, SurfaceError> {
    combine_parts(cylinder_parts_range(
        radius,
        height,
        segments,
        show_ends,
        angle_range,
    )?)
}

/// Cylinder side and, when requested, lower and upper caps as independent parts.
pub fn cylinder_parts(
    radius: f64,
    height: f64,
    segments: usize,
) -> Result<Vec<MeshResource>, SurfaceError> {
    cylinder_parts_range(radius, height, segments, true, [0.0, std::f64::consts::TAU])
}

pub fn cylinder_parts_range(
    radius: f64,
    height: f64,
    segments: usize,
    show_ends: bool,
    angle_range: [f64; 2],
) -> Result<Vec<MeshResource>, SurfaceError> {
    positive(radius)?;
    positive(height)?;
    check_segments(segments)?;
    validate_angle_range(angle_range)?;
    let side = UvSurfacePlan::new(angle_range, [0.0, height], [segments, 1])?
        .sample(|angle, z| {
            let (sin, cos) = angle.sin_cos();
            let normal = SemanticVec3::new(cos, sin, 0.0);
            SurfaceSample::with_normal(SemanticVec3::new(radius * cos, radius * sin, z), normal)
        })?
        .into_mesh_resource()?;
    let mut parts = Vec::with_capacity(if show_ends { 3 } else { 1 });
    parts.push(side);
    if show_ends {
        parts.push(cap_mesh(radius, 0.0, segments, -1.0)?);
        parts.push(cap_mesh(radius, height, segments, 1.0)?);
    }
    Ok(parts)
}

pub fn cone_mesh(radius: f64, height: f64, segments: usize) -> Result<MeshResource, SurfaceError> {
    cone_mesh_range(radius, height, segments, true, [0.0, std::f64::consts::TAU])
}

/// Build a cone side over one bounded azimuth interval. Optional base geometry
/// keeps the full-circle `show_base` behavior used by Manim v0.21.
pub fn cone_mesh_range(
    radius: f64,
    height: f64,
    segments: usize,
    show_base: bool,
    angle_range: [f64; 2],
) -> Result<MeshResource, SurfaceError> {
    combine_parts(cone_parts_range(
        radius,
        height,
        segments,
        show_base,
        angle_range,
    )?)
}

/// Cone side and optional lower base as independent parts.
pub fn cone_parts(
    radius: f64,
    height: f64,
    segments: usize,
) -> Result<Vec<MeshResource>, SurfaceError> {
    cone_parts_range(radius, height, segments, true, [0.0, std::f64::consts::TAU])
}

pub fn cone_parts_range(
    radius: f64,
    height: f64,
    segments: usize,
    show_base: bool,
    angle_range: [f64; 2],
) -> Result<Vec<MeshResource>, SurfaceError> {
    positive(radius)?;
    positive(height)?;
    check_segments(segments)?;
    validate_angle_range(angle_range)?;
    let side = UvSurfacePlan::new(angle_range, [0.0, height], [segments, 1])?
        .sample(|angle, z| {
            let fraction = (1.0 - z / height).max(0.0);
            let (sin, cos) = angle.sin_cos();
            let normal = normalize(SemanticVec3::new(height * cos, height * sin, radius))
                .unwrap_or(SemanticVec3::new(0.0, 0.0, 1.0));
            SurfaceSample::with_normal(
                SemanticVec3::new(radius * fraction * cos, radius * fraction * sin, z),
                normal,
            )
        })?
        .into_mesh_resource()?;
    let mut parts = Vec::with_capacity(if show_base { 2 } else { 1 });
    parts.push(side);
    if show_base {
        parts.push(cap_mesh(radius, 0.0, segments, -1.0)?);
    }
    Ok(parts)
}

/// Rectangular prism with flat-shaded, independently indexed faces.
pub fn prism_mesh(size: SemanticVec3) -> Result<MeshResource, SurfaceError> {
    combine_parts(prism_faces(size)?)
}

/// The six flat-shaded faces in +X, -X, +Y, -Y, +Z, -Z order.
pub fn prism_faces(size: SemanticVec3) -> Result<[MeshResource; 6], SurfaceError> {
    if !size.is_finite() {
        return Err(SurfaceError::NonFinitePosition);
    }
    if size.x <= 0.0 || size.y <= 0.0 || size.z <= 0.0 {
        return Err(SurfaceError::InvalidRange);
    }
    let h = SemanticVec3::new(size.x * 0.5, size.y * 0.5, size.z * 0.5);
    if h.x == 0.0 || h.y == 0.0 || h.z == 0.0 {
        return Err(SurfaceError::InvalidRange);
    }
    let mut builder = MeshBuilder::default();
    let mut faces = Vec::with_capacity(6);
    // Each [a,b,c,d] face uses the same a-b-d / b-c-d CCW topology as UV cells.
    builder.add_quad(
        [
            SemanticVec3::new(h.x, -h.y, -h.z),
            SemanticVec3::new(h.x, h.y, -h.z),
            SemanticVec3::new(h.x, h.y, h.z),
            SemanticVec3::new(h.x, -h.y, h.z),
        ],
        SemanticVec3::new(1.0, 0.0, 0.0),
    )?;
    faces.push(builder.take_mesh()?);
    builder.add_quad(
        [
            SemanticVec3::new(-h.x, -h.y, -h.z),
            SemanticVec3::new(-h.x, -h.y, h.z),
            SemanticVec3::new(-h.x, h.y, h.z),
            SemanticVec3::new(-h.x, h.y, -h.z),
        ],
        SemanticVec3::new(-1.0, 0.0, 0.0),
    )?;
    faces.push(builder.take_mesh()?);
    builder.add_quad(
        [
            SemanticVec3::new(-h.x, h.y, -h.z),
            SemanticVec3::new(-h.x, h.y, h.z),
            SemanticVec3::new(h.x, h.y, h.z),
            SemanticVec3::new(h.x, h.y, -h.z),
        ],
        SemanticVec3::new(0.0, 1.0, 0.0),
    )?;
    faces.push(builder.take_mesh()?);
    builder.add_quad(
        [
            SemanticVec3::new(-h.x, -h.y, -h.z),
            SemanticVec3::new(h.x, -h.y, -h.z),
            SemanticVec3::new(h.x, -h.y, h.z),
            SemanticVec3::new(-h.x, -h.y, h.z),
        ],
        SemanticVec3::new(0.0, -1.0, 0.0),
    )?;
    faces.push(builder.take_mesh()?);
    builder.add_quad(
        [
            SemanticVec3::new(-h.x, -h.y, h.z),
            SemanticVec3::new(h.x, -h.y, h.z),
            SemanticVec3::new(h.x, h.y, h.z),
            SemanticVec3::new(-h.x, h.y, h.z),
        ],
        SemanticVec3::new(0.0, 0.0, 1.0),
    )?;
    faces.push(builder.take_mesh()?);
    builder.add_quad(
        [
            SemanticVec3::new(-h.x, -h.y, -h.z),
            SemanticVec3::new(-h.x, h.y, -h.z),
            SemanticVec3::new(h.x, h.y, -h.z),
            SemanticVec3::new(h.x, -h.y, -h.z),
        ],
        SemanticVec3::new(0.0, 0.0, -1.0),
    )?;
    faces.push(builder.finish()?);
    faces.try_into().map_err(|_| SurfaceError::SurfaceTooLarge)
}

pub fn cube_mesh(size: f64) -> Result<MeshResource, SurfaceError> {
    positive(size)?;
    prism_mesh(SemanticVec3::new(size, size, size))
}

#[derive(Default)]
struct MeshBuilder {
    positions: Vec<SemanticVec3>,
    normals: Vec<SemanticVec3>,
    indices: Vec<u32>,
}

impl MeshBuilder {
    fn take_mesh(&mut self) -> Result<MeshResource, SurfaceError> {
        std::mem::take(self).finish()
    }

    fn append(&mut self, mesh: &MeshResource) -> Result<(), SurfaceError> {
        let base =
            u32::try_from(self.positions.len()).map_err(|_| SurfaceError::SurfaceTooLarge)?;
        let end = self
            .positions
            .len()
            .checked_add(mesh.positions().len())
            .filter(|&n| n <= MAX_SURFACE_VERTICES)
            .ok_or(SurfaceError::SurfaceTooLarge)?;
        if end > u32::MAX as usize {
            return Err(SurfaceError::SurfaceTooLarge);
        }
        self.positions.extend_from_slice(mesh.positions());
        let normals = mesh.normals().ok_or(SurfaceError::DegenerateNormal)?;
        self.normals.extend_from_slice(normals);
        for &index in mesh.indices() {
            self.indices.push(
                base.checked_add(index)
                    .ok_or(SurfaceError::SurfaceTooLarge)?,
            );
        }
        Ok(())
    }

    fn add_quad(
        &mut self,
        points: [SemanticVec3; 4],
        normal: SemanticVec3,
    ) -> Result<(), SurfaceError> {
        let base =
            u32::try_from(self.positions.len()).map_err(|_| SurfaceError::SurfaceTooLarge)?;
        self.positions.extend(points);
        self.normals.extend([normal; 4]);
        self.indices
            .extend([base, base + 1, base + 3, base + 1, base + 2, base + 3]);
        Ok(())
    }

    fn add_cap(
        &mut self,
        radius: f64,
        z: f64,
        segments: usize,
        normal_z: f64,
    ) -> Result<(), SurfaceError> {
        let base =
            u32::try_from(self.positions.len()).map_err(|_| SurfaceError::SurfaceTooLarge)?;
        let center = SemanticVec3::new(0.0, 0.0, z);
        let normal = SemanticVec3::new(0.0, 0.0, normal_z);
        self.positions.push(center);
        self.normals.push(normal);
        for index in 0..segments {
            let angle = std::f64::consts::TAU * index as f64 / segments as f64;
            let (sin, cos) = angle.sin_cos();
            self.positions
                .push(SemanticVec3::new(radius * cos, radius * sin, z));
            self.normals.push(normal);
        }
        for index in 0..segments {
            let current = base + 1 + index as u32;
            let next = base + 1 + ((index + 1) % segments) as u32;
            if normal_z > 0.0 {
                self.indices.extend([base, current, next]);
            } else {
                self.indices.extend([base, next, current]);
            }
        }
        Ok(())
    }

    fn finish(self) -> Result<MeshResource, SurfaceError> {
        MeshResource::new(self.positions, Some(self.normals), self.indices).map_err(Into::into)
    }
}

fn cap_mesh(
    radius: f64,
    z: f64,
    segments: usize,
    normal_z: f64,
) -> Result<MeshResource, SurfaceError> {
    let mut builder = MeshBuilder::default();
    builder.add_cap(radius, z, segments, normal_z)?;
    builder.finish()
}

fn combine_parts(
    parts: impl IntoIterator<Item = MeshResource>,
) -> Result<MeshResource, SurfaceError> {
    let mut builder = MeshBuilder::default();
    for part in parts {
        builder.append(&part)?;
    }
    builder.finish()
}

fn positive(value: f64) -> Result<(), SurfaceError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(SurfaceError::InvalidRange)
    }
}

fn check_segments(segments: usize) -> Result<(), SurfaceError> {
    if segments < 3 {
        return Err(SurfaceError::InvalidResolution);
    }
    if segments > MAX_SURFACE_CELLS / 6 {
        return Err(SurfaceError::SurfaceTooLarge);
    }
    Ok(())
}

fn validate_angle_range(range: [f64; 2]) -> Result<(), SurfaceError> {
    if range[0].is_finite()
        && range[1].is_finite()
        && range[0] >= 0.0
        && range[0] < range[1]
        && range[1] <= std::f64::consts::TAU
    {
        Ok(())
    } else {
        Err(SurfaceError::InvalidRange)
    }
}

fn scale(value: SemanticVec3, factor: f64) -> SemanticVec3 {
    SemanticVec3::new(value.x * factor, value.y * factor, value.z * factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle_normal(mesh: &MeshResource, triangle: usize) -> SemanticVec3 {
        let i = &mesh.indices()[triangle * 3..triangle * 3 + 3];
        let p = mesh.positions();
        let a = p[i[0] as usize];
        let b = p[i[1] as usize];
        let c = p[i[2] as usize];
        crate::surface::cross(
            crate::surface::subtract(b, a),
            crate::surface::subtract(c, a),
        )
    }

    #[test]
    fn generated_spheres_and_tori_have_deterministic_seamed_topology() {
        let sphere = sphere_mesh(2.0, [8, 12]).unwrap();
        assert_eq!(sphere.positions().len(), 9 * 13);
        assert_eq!(sphere.indices().len(), 8 * 12 * 6);
        assert_eq!(sphere.bounds().min, SemanticVec3::new(-2.0, -2.0, -2.0));
        assert_eq!(sphere.bounds().max, SemanticVec3::new(2.0, 2.0, 2.0));
        let seam_delta =
            crate::surface::subtract(sphere.positions()[0], sphere.positions()[8 * 13]);
        assert!(seam_delta.x.abs() + seam_delta.y.abs() + seam_delta.z.abs() < 1.0e-14);
        assert_eq!(
            sphere.normals().unwrap()[0],
            SemanticVec3::new(0.0, 0.0, -1.0)
        );
        let normal = triangle_normal(&sphere, 2 * 5);
        assert!(normal.x > 0.0, "Manim sphere triangles face outward");
        let torus = torus_mesh(3.0, 1.0, [12, 8]).unwrap();
        assert_eq!(torus.positions().len(), 13 * 9);
        assert_eq!(torus.indices().len(), 12 * 8 * 6);
        assert!(torus.bounds().min.x <= -4.0 && torus.bounds().max.x >= 4.0);
        assert!(triangle_normal(&torus, 0).x > 0.0);
    }

    #[test]
    fn sphere_ranges_generate_an_open_outward_patch_and_reject_invalid_domains() {
        let sphere = sphere_mesh_range(1.0, [8, 6], [0.2, 1.4], [0.3, 2.7]).unwrap();
        assert_eq!(sphere.positions().len(), 9 * 7);
        assert_eq!(sphere.indices().len(), 8 * 6 * 6);
        assert!(sphere.bounds().min.x > -1.0);
        assert!(sphere.bounds().max.x < 1.0);
        let position = sphere.positions()[3 * 7 + 3];
        let normal = sphere.normals().unwrap()[3 * 7 + 3];
        assert!((position.x - normal.x).abs() < 1.0e-14);
        assert!((position.y - normal.y).abs() < 1.0e-14);
        assert!((position.z - normal.z).abs() < 1.0e-14);
        let triangle = triangle_normal(&sphere, (3 * 6 + 3) * 2);
        let outward = triangle.x * normal.x + triangle.y * normal.y + triangle.z * normal.z;
        assert!(outward > 0.0);

        for (u_range, v_range) in [
            ([-0.1, 1.0], [0.0, std::f64::consts::PI]),
            (
                [0.0, std::f64::consts::TAU + 0.1],
                [0.0, std::f64::consts::PI],
            ),
            ([0.0, std::f64::consts::TAU], [-0.1, 1.0]),
            (
                [0.0, std::f64::consts::TAU],
                [0.0, std::f64::consts::PI + 0.1],
            ),
            ([0.5, 0.5], [0.0, std::f64::consts::PI]),
        ] {
            assert_eq!(
                sphere_mesh_range(1.0, [8, 8], u_range, v_range),
                Err(SurfaceError::InvalidRange)
            );
        }
        assert_eq!(
            sphere_mesh_range(1.0, [MAX_SURFACE_CELLS, 2], [0.0, 1.0], [0.0, 1.0]),
            Err(SurfaceError::SurfaceTooLarge)
        );
    }

    #[test]
    fn cylinder_cone_caps_and_prism_faces_have_outward_winding() {
        let cylinder = cylinder_mesh(2.0, 3.0, 12).unwrap();
        let cylinder_parts = cylinder_parts(2.0, 3.0, 12).unwrap();
        assert_eq!(cylinder_parts[0].indices().len(), 12 * 6);
        assert_eq!(cylinder_parts[1].indices().len(), 12 * 3);
        assert_eq!(cylinder_parts[2].indices().len(), 12 * 3);
        assert_eq!(cylinder.positions().len(), 2 * 13 + 2 * 13);
        assert_eq!(cylinder.indices().len(), 12 * 6 + 2 * 12 * 3);
        assert_eq!(cylinder.bounds().min, SemanticVec3::new(-2.0, -2.0, 0.0));
        assert_eq!(cylinder.bounds().max, SemanticVec3::new(2.0, 2.0, 3.0));
        let side_normal = triangle_normal(&cylinder, 0);
        assert!(
            side_normal.x > 0.0,
            "side winding must face outward: {side_normal:?}"
        );
        // The first appended cap follows the side's 2*(segments+1) vertices.
        let cap_triangle = 12 * 2;
        assert!(triangle_normal(&cylinder, cap_triangle).z < 0.0);
        assert!(triangle_normal(&cylinder, cap_triangle + 12).z > 0.0);

        let cone = cone_mesh(1.0, 2.0, 12).unwrap();
        let cone_parts = cone_parts(1.0, 2.0, 12).unwrap();
        assert_eq!(cone_parts[0].indices().len(), 12 * 6);
        assert_eq!(cone_parts[1].indices().len(), 12 * 3);
        assert_eq!(cone.indices().len(), 12 * 6 + 12 * 3);
        assert_eq!(cone.bounds().max.z, 2.0);
        assert!(triangle_normal(&cone, 0).x > 0.0);
        assert!(triangle_normal(&cone, 12 * 2).z < 0.0);
        let prism = prism_mesh(SemanticVec3::new(2.0, 4.0, 6.0)).unwrap();
        let faces = prism_faces(SemanticVec3::new(2.0, 4.0, 6.0)).unwrap();
        assert_eq!(prism.positions().len(), 24);
        assert_eq!(prism.indices().len(), 36);
        assert!(faces.iter().all(|face| face.indices().len() == 6));
        let expected = [
            SemanticVec3::new(1.0, 0.0, 0.0),
            SemanticVec3::new(-1.0, 0.0, 0.0),
            SemanticVec3::new(0.0, 1.0, 0.0),
            SemanticVec3::new(0.0, -1.0, 0.0),
            SemanticVec3::new(0.0, 0.0, 1.0),
            SemanticVec3::new(0.0, 0.0, -1.0),
        ];
        for (face, expected_normal) in expected.into_iter().enumerate() {
            let normal = triangle_normal(&prism, face * 2);
            let dot = normal.x * expected_normal.x
                + normal.y * expected_normal.y
                + normal.z * expected_normal.z;
            assert!(dot > 0.0, "face {face} winding must be outward: {normal:?}");
        }
    }

    #[test]
    fn open_cylinder_and_cone_retain_only_their_side_topology() {
        let segments = 12;
        let cylinder =
            cylinder_mesh_range(2.0, 3.0, segments, false, [0.0, std::f64::consts::TAU]).unwrap();
        let cylinder_parts =
            cylinder_parts_range(2.0, 3.0, segments, false, [0.0, std::f64::consts::TAU]).unwrap();
        assert_eq!(cylinder_parts.len(), 1);
        assert_eq!(cylinder.positions().len(), 2 * (segments + 1));
        assert_eq!(cylinder.indices().len(), segments * 6);
        assert!(cylinder.has_usable_normals());
        assert_eq!(cylinder.bounds().min.z, 0.0);
        assert_eq!(cylinder.bounds().max.z, 3.0);
        assert!(cylinder
            .normals()
            .unwrap()
            .iter()
            .all(|normal| normal.z == 0.0));
        assert!(triangle_normal(&cylinder, 0).x > 0.0);

        let cone =
            cone_mesh_range(1.0, 2.0, segments, false, [0.0, std::f64::consts::TAU]).unwrap();
        let cone_parts =
            cone_parts_range(1.0, 2.0, segments, false, [0.0, std::f64::consts::TAU]).unwrap();
        assert_eq!(cone_parts.len(), 1);
        assert_eq!(cone.positions().len(), 2 * (segments + 1));
        assert_eq!(cone.indices().len(), segments * 6);
        assert!(cone.has_usable_normals());
        assert_eq!(cone.bounds().max.z, 2.0);
        assert!(triangle_normal(&cone, 0).x > 0.0);

        let closed_cylinder = cylinder_mesh(2.0, 3.0, segments).unwrap();
        let closed_cone = cone_mesh(1.0, 2.0, segments).unwrap();
        assert_eq!(
            closed_cylinder.indices().len(),
            cylinder.indices().len() + 2 * segments * 3
        );
        assert_eq!(
            closed_cone.indices().len(),
            cone.indices().len() + segments * 3
        );
    }

    #[test]
    fn cylinder_and_cone_partial_azimuths_keep_bounded_wound_side_topology() {
        let segments = 8;
        let angle_range = [0.0, std::f64::consts::FRAC_PI_2];
        let cylinder = cylinder_mesh_range(2.0, 3.0, segments, false, angle_range).unwrap();
        assert_eq!(cylinder.positions().len(), 2 * (segments + 1));
        assert_eq!(cylinder.indices().len(), segments * 6);
        assert!(cylinder.bounds().min.x.abs() < 1e-12);
        assert!(cylinder.bounds().min.y.abs() < 1e-12);
        assert!((cylinder.bounds().max.x - 2.0).abs() < 1e-12);
        assert!((cylinder.bounds().max.y - 2.0).abs() < 1e-12);
        assert!(triangle_normal(&cylinder, 0).x > 0.0);
        assert!(cylinder.has_usable_normals());

        let cone = cone_mesh_range(1.0, 2.0, segments, false, angle_range).unwrap();
        assert_eq!(cone.positions().len(), 2 * (segments + 1));
        assert_eq!(cone.indices().len(), segments * 6);
        assert!(cone.bounds().min.x.abs() < 1e-12);
        assert!(cone.bounds().min.y.abs() < 1e-12);
        assert!((cone.bounds().max.x - 1.0).abs() < 1e-12);
        assert!((cone.bounds().max.y - 1.0).abs() < 1e-12);
        assert!(triangle_normal(&cone, 0).x > 0.0);
        assert!(cone.has_usable_normals());

        // Manim's show_ends/show_base add complete end disks even when the
        // sampled side is a partial azimuth patch.
        let capped_cylinder = cylinder_parts_range(2.0, 3.0, segments, true, angle_range).unwrap();
        assert_eq!(capped_cylinder.len(), 3);
        assert!((capped_cylinder[1].bounds().max.x - 2.0).abs() < 1e-12);
        let capped_cone = cone_parts_range(1.0, 2.0, segments, true, angle_range).unwrap();
        assert_eq!(capped_cone.len(), 2);
        assert!((capped_cone[1].bounds().max.x - 1.0).abs() < 1e-12);

        for invalid in [
            [-0.1, 1.0],
            [1.0, 1.0],
            [2.0, 1.0],
            [0.0, std::f64::consts::TAU + 0.1],
            [0.0, f64::INFINITY],
        ] {
            assert_eq!(
                cylinder_mesh_range(1.0, 2.0, segments, false, invalid),
                Err(SurfaceError::InvalidRange)
            );
            assert_eq!(
                cone_mesh_range(1.0, 2.0, segments, false, invalid),
                Err(SurfaceError::InvalidRange)
            );
        }
    }

    #[test]
    fn rejects_degenerate_or_unbounded_solids_before_allocation() {
        assert_eq!(
            sphere_mesh_range(
                0.0,
                [8, 8],
                [0.0, std::f64::consts::TAU],
                [0.0, std::f64::consts::PI]
            ),
            Err(SurfaceError::InvalidRange)
        );
        assert_eq!(
            torus_mesh(1.0, 1.0, [8, 8]),
            Err(SurfaceError::InvalidRange)
        );
        assert_eq!(
            cylinder_mesh(1.0, f64::INFINITY, 12),
            Err(SurfaceError::InvalidRange)
        );
        assert_eq!(
            cone_mesh(1.0, 1.0, usize::MAX),
            Err(SurfaceError::SurfaceTooLarge)
        );
        assert_eq!(
            prism_mesh(SemanticVec3::new(1.0, 0.0, 1.0)),
            Err(SurfaceError::InvalidRange)
        );
    }
}
