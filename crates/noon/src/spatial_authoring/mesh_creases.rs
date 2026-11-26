//! Classify the authored triangle adjacency before the face mesh splits its vertices.

use super::{sketch_edge, validate_sketch_edge, MeshOptions, SpatialPathOptions};
use crate::AuthoringError;
use noon_core::{
    Color, SemanticPaint, SemanticRotation3D, SemanticSpatialMaterial, SemanticVec3,
    SemanticWorldTransform3D, SilhouetteEdge, SketchStroke, Vec2, VectorPath,
};
use std::collections::{btree_map::Entry, BTreeMap};

const INVALID_TOPOLOGY: AuthoringError = AuthoringError::InvalidSketchStroke(
    "crease strokes require closed indexed triangles with consistent winding",
);

struct Edge {
    normal: SemanticVec3,
    forward: bool,
    neighbor: Option<SemanticVec3>,
}

pub(crate) fn polyhedron_sketch_options(
    vertices: &[SemanticVec3],
    faces: &[[u32; 3]],
    fill: Color,
    stroke: Color,
    width: f64,
    sketch: SketchStroke,
    silhouette: bool,
) -> Result<(MeshOptions, Vec<SpatialPathOptions>), AuthoringError> {
    validate_sketch_edge(stroke, width, sketch)?;
    if fill.alpha != 1.0 {
        return Err(AuthoringError::InvalidSketchStroke(
            "polyhedron crease strokes require an opaque fill",
        ));
    }
    let geometry = noon_geometry::triangular_polyhedron_mesh(vertices, faces)
        .map_err(|_| AuthoringError::NonFiniteGeometry)?;
    let normals = geometry.normals().expect("polyhedron has flat normals");
    let mut edges = BTreeMap::new();
    for (&[a, b, c], normal) in faces.iter().zip(normals.as_chunks::<3>().0) {
        for (start, end) in [(a, b), (b, c), (c, a)] {
            let forward = start < end;
            match edges.entry((start.min(end), start.max(end))) {
                Entry::Vacant(entry) => {
                    entry.insert(Edge {
                        normal: normal[0],
                        forward,
                        neighbor: None,
                    });
                }
                Entry::Occupied(mut entry) => {
                    let edge = entry.get_mut();
                    if edge.forward == forward || edge.neighbor.replace(normal[0]).is_some() {
                        return Err(INVALID_TOPOLOGY);
                    }
                }
            }
        }
    }

    let mut paths = Vec::new();
    for ((a, b), edge) in edges {
        let first = edge.normal;
        let second = edge.neighbor.ok_or(INVALID_TOPOLOGY)?;
        // Nearly parallel face normals represent a coplanar triangulation seam.
        let cosine = first.x * second.x + first.y * second.y + first.z * second.z;
        if cosine >= 1.0 - 1.0e-8 {
            continue;
        }
        let (path, transform) =
            crease_path(vertices[a as usize], vertices[b as usize], first, second)
                .ok_or(AuthoringError::NonFiniteGeometry)?;
        let mut edge = sketch_edge(path, transform, stroke, width, sketch, paths.len() as u32);
        if silhouette {
            edge.material = SemanticSpatialMaterial::SketchSilhouette(
                SilhouetteEdge::from_face_normal_dot(cosine)
                    .ok_or(AuthoringError::NonFiniteGeometry)?,
            );
        }
        paths.push(edge);
    }
    let mut mesh = MeshOptions::new(geometry);
    mesh.style.fill = Some(SemanticPaint::Solid(fill));
    Ok((mesh, paths))
}

fn crease_path(
    start: SemanticVec3,
    end: SemanticVec3,
    first: SemanticVec3,
    second: SemanticVec3,
) -> Option<(VectorPath, SemanticWorldTransform3D)> {
    let edge = SemanticVec3::new(end.x - start.x, end.y - start.y, end.z - start.z);
    let half = (edge.x.hypot(edge.y).hypot(edge.z) * 0.5) as f32;
    if half <= 0.0 || !half.is_finite() {
        return None;
    }
    // Place the edge in the plane perpendicular to the sum of its face normals.
    // On a convex crease this lets both sides of the wavy stroke clear the solid.
    let normal = SemanticVec3::new(first.x + second.x, first.y + second.y, first.z + second.z);
    let plane = SemanticWorldTransform3D::from_axial_direction(normal, 0.0)?.rotation;
    let local = plane.inverse().rotate_vector(edge)?;
    let align = SemanticRotation3D::from_axis_angle(
        SemanticVec3::new(0.0, 0.0, 1.0),
        local.y.atan2(local.x),
    )?;
    let transform = SemanticWorldTransform3D {
        translation: SemanticVec3::new(
            start.x + edge.x * 0.5,
            start.y + edge.y * 0.5,
            start.z + edge.z * 0.5,
        ),
        rotation: plane.compose(align)?,
        ..SemanticWorldTransform3D::IDENTITY
    };
    Some((
        VectorPath::new()
            .move_to(Vec2::new(-half, 0.0))
            .line_to(Vec2::new(half, 0.0)),
        transform,
    ))
}

#[cfg(test)]
#[test]
fn mesh_sketch_creases_require_closed_consistent_triangle_adjacency() {
    let points = [
        SemanticVec3::ZERO,
        SemanticVec3::new(1.0, 0.0, 0.0),
        SemanticVec3::new(0.0, 1.0, 0.0),
        SemanticVec3::new(0.0, 0.0, 1.0),
    ];
    let faces = [[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]];
    let options = |faces: &[[u32; 3]]| {
        polyhedron_sketch_options(
            &points,
            faces,
            Color::BLUE,
            Color::WHITE,
            0.04,
            SketchStroke::new(1.0, 7).unwrap(),
            false,
        )
    };
    assert_eq!(options(&faces).unwrap().1.len(), 6);
    assert!(matches!(
        options(&faces[..3]),
        Err(AuthoringError::InvalidSketchStroke(_))
    ));
    let mut inconsistent = faces;
    inconsistent[0].swap(1, 2);
    assert!(matches!(
        options(&inconsistent),
        Err(AuthoringError::InvalidSketchStroke(_))
    ));
}
