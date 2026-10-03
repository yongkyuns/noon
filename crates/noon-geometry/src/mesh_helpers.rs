//! Common bounded spatial helpers built from the same indexed mesh resources.
use crate::surface::normalize;
use crate::{cylinder_mesh, SurfaceError, MAX_SURFACE_CELLS, MAX_SURFACE_VERTICES};
use noon_core::{MeshResource, SemanticRotation3D, SemanticVec3};

/// A closed cylindrical line with world-space endpoints. Thickness is its diameter.
/// Geometry is prepared once; ordinary world transforms animate the retained resource.
pub fn line_3d_mesh(
    start: SemanticVec3,
    end: SemanticVec3,
    thickness: f64,
    segments: usize,
) -> Result<MeshResource, SurfaceError> {
    if !start.is_finite() || !end.is_finite() {
        return Err(SurfaceError::NonFinitePosition);
    }
    let delta = SemanticVec3::new(end.x - start.x, end.y - start.y, end.z - start.z);
    let length = delta.x.hypot(delta.y).hypot(delta.z);
    if !length.is_finite() || length <= 0.0 || !thickness.is_finite() || thickness <= 0.0 {
        return Err(SurfaceError::InvalidRange);
    }
    let direction = normalize(delta).ok_or(SurfaceError::InvalidRange)?;
    let sine = direction.x.hypot(direction.y);
    let rotation = if sine > 0.0 {
        SemanticRotation3D::from_axis_angle(
            SemanticVec3::new(-direction.y, direction.x, 0.0),
            sine.atan2(direction.z),
        )
    } else if direction.z >= 0.0 {
        Some(SemanticRotation3D::IDENTITY)
    } else {
        SemanticRotation3D::from_axis_angle(SemanticVec3::new(1.0, 0.0, 0.0), std::f64::consts::PI)
    }
    .ok_or(SurfaceError::NonFiniteNormal)?;
    let mesh = cylinder_mesh(thickness * 0.5, length, segments)?;
    let positions = mesh
        .positions()
        .iter()
        .map(|&p| {
            let p = rotation
                .rotate_vector(p)
                .ok_or(SurfaceError::NonFinitePosition)?;
            let p = SemanticVec3::new(p.x + start.x, p.y + start.y, p.z + start.z);
            p.is_finite()
                .then_some(p)
                .ok_or(SurfaceError::NonFinitePosition)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let normals = mesh
        .normals()
        .ok_or(SurfaceError::DegenerateNormal)?
        .iter()
        .map(|&n| {
            rotation
                .rotate_vector(n)
                .ok_or(SurfaceError::NonFiniteNormal)
        })
        .collect::<Result<Vec<_>, _>>()?;
    MeshResource::new(positions, Some(normals), mesh.indices().to_vec()).map_err(Into::into)
}

/// An explicitly wound triangular polyhedron with a flat normal per face.
/// Faces need not be closed; no implicit hull construction, triangulation or winding repair occurs.
pub fn triangular_polyhedron_mesh(
    vertices: &[SemanticVec3],
    faces: &[[u32; 3]],
) -> Result<MeshResource, SurfaceError> {
    if faces.is_empty() || vertices.is_empty() {
        return Err(SurfaceError::InvalidResolution);
    }
    let count = faces
        .len()
        .checked_mul(3)
        .filter(|&n| n <= MAX_SURFACE_VERTICES && faces.len() <= MAX_SURFACE_CELLS)
        .ok_or(SurfaceError::SurfaceTooLarge)?;
    if vertices.len() > MAX_SURFACE_VERTICES {
        return Err(SurfaceError::SurfaceTooLarge);
    }
    if vertices.iter().any(|p| !p.is_finite()) {
        return Err(SurfaceError::NonFinitePosition);
    }
    let mut positions = Vec::with_capacity(count);
    let mut normals = Vec::with_capacity(count);
    let mut indices = Vec::with_capacity(count);
    for face in faces {
        let [a, b, c] = face.map(|i| vertices.get(i as usize).copied());
        let (Some(a), Some(b), Some(c)) = (a, b, c) else {
            return Err(SurfaceError::InvalidRange);
        };
        let ab = normalize(SemanticVec3::new(b.x - a.x, b.y - a.y, b.z - a.z))
            .ok_or(SurfaceError::DegenerateNormal)?;
        let ac = normalize(SemanticVec3::new(c.x - a.x, c.y - a.y, c.z - a.z))
            .ok_or(SurfaceError::DegenerateNormal)?;
        let normal = normalize(SemanticVec3::new(
            ab.y * ac.z - ab.z * ac.y,
            ab.z * ac.x - ab.x * ac.z,
            ab.x * ac.y - ab.y * ac.x,
        ))
        .ok_or(SurfaceError::DegenerateNormal)?;
        let base = u32::try_from(positions.len()).map_err(|_| SurfaceError::SurfaceTooLarge)?;
        positions.extend([a, b, c]);
        normals.extend([normal; 3]);
        indices.extend([base, base + 1, base + 2]);
    }
    MeshResource::new(positions, Some(normals), indices).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lines_preserve_cap_centers_for_oblique_and_negative_z_directions() {
        let start = SemanticVec3::new(1.0, -2.0, 3.0);
        for end in [
            SemanticVec3::new(4.0, 2.0, 8.0),
            SemanticVec3::new(1.0, -2.0, -4.0),
            SemanticVec3::new(1.0, -2.0, 9.0),
        ] {
            let mesh = line_3d_mesh(start, end, 0.2, 8).unwrap();
            let p = mesh.positions();
            // Side: two samples for each of the nine angle positions; caps then contain centers.
            for (actual, expected) in [(p[18], start), (p[27], end)] {
                assert!((actual.x - expected.x).abs() < 1e-12);
                assert!((actual.y - expected.y).abs() < 1e-12);
                assert!((actual.z - expected.z).abs() < 1e-12);
            }
            assert!(mesh.has_usable_normals());
        }
        assert!(line_3d_mesh(start, start, 0.2, 8).is_err());
        assert!(line_3d_mesh(start, SemanticVec3::ZERO, 0.0, 8).is_err());
    }
    #[test]
    fn polyhedron_faces_have_independent_flat_normals_and_checked_topology() {
        let vertices = [
            SemanticVec3::ZERO,
            SemanticVec3::new(1.0, 0.0, 0.0),
            SemanticVec3::new(0.0, 1.0, 0.0),
            SemanticVec3::new(0.0, 0.0, 1.0),
        ];
        let mesh = triangular_polyhedron_mesh(&vertices, &[[0, 1, 2], [0, 3, 1]]).unwrap();
        assert_eq!(mesh.positions().len(), 6);
        assert_eq!(mesh.normals().unwrap()[0], SemanticVec3::new(0.0, 0.0, 1.0));
        assert_eq!(mesh.normals().unwrap()[3], SemanticVec3::new(0.0, 1.0, 0.0));
        assert!(triangular_polyhedron_mesh(&vertices, &[[0, 1, 2], [0, 2, 99]]).is_err());
        assert!(triangular_polyhedron_mesh(&vertices, &[[0, 1, 1]]).is_err());
    }
}
