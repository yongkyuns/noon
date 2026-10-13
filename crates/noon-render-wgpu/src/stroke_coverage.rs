//! Renderer-local pixel coverage for bounded, static stroke meshes.
//!
//! Recover convex pieces from the existing tessellation, preserving its fill,
//! stroke order and reveal triangulation. Projection and pixel coverage remain
//! in the shared GPU path; no camera-dependent retessellation is needed.
use crate::{
    pack_path_surface, PathVertex, POLYGON_CORNER_SHIFT, POLYGON_COVERAGE_FLAG, POLYGON_QUAD_FLAG,
    POLYGON_TIGHT_FLAG, TRIANGLE_QUAD_INDICES,
};
use noon_geometry::{PathSurface, TessellatedPath};

// Keep preparation bounded to small retained meshes. Larger/curved/sampled
// geometry keeps the ordinary tessellation path and its existing batching.
const MAX_COVERAGE_VERTICES: usize = 128;
const MAX_COVERAGE_INDICES: usize = 768;

pub(crate) fn eligible(mesh: &TessellatedPath) -> bool {
    mesh.has_polygonal_stroke()
        && !mesh.morphing
        && mesh.vertices.len() <= MAX_COVERAGE_VERTICES
        && mesh.indices.len() <= MAX_COVERAGE_INDICES
        && !mesh.indices.is_empty()
        && mesh.indices.len().is_multiple_of(3)
        && mesh
            .vertices
            .iter()
            .any(|v| v.surface == PathSurface::Stroke)
        && mesh.vertices.iter().all(|v| {
            v.position == v.target_position
                && v.position.x.is_finite()
                && v.position.y.is_finite()
                && v.path_progress.is_finite()
        })
        && mesh
            .indices
            .iter()
            .all(|&i| (i as usize) < mesh.vertices.len())
        && mesh.indices.as_chunks::<3>().0.iter().all(|t| {
            let surface = mesh.vertices[t[0] as usize].surface;
            t.iter()
                .all(|&i| mesh.vertices[i as usize].surface == surface)
        })
}

fn cross(a: noon_core::Vec2, b: noon_core::Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

fn convex_pair(mesh: &TessellatedPath, a: [u32; 3], b: [u32; 3]) -> Option<[u32; 4]> {
    let shared = a.into_iter().filter(|i| b.contains(i)).collect::<Vec<_>>();
    if shared.len() != 2 {
        return None;
    }
    let first = a.into_iter().find(|i| !shared.contains(i))?;
    let last = b.into_iter().find(|i| !shared.contains(i))?;
    // The original common diagonal remains (0,2), preserving partially revealed
    // path-progress interpolation when the analytic coverage is disabled.
    let order = [shared[0], first, shared[1], last];
    let points = order.map(|i| mesh.vertices[i as usize].position);
    let mut winding = 0.0_f32;
    for i in 0..4 {
        let turn = cross(
            points[(i + 1) % 4] - points[i],
            points[(i + 2) % 4] - points[(i + 1) % 4],
        );
        if !turn.is_finite() || turn == 0.0 {
            return None;
        }
        if winding != 0.0 && turn.signum() != winding.signum() {
            return None;
        }
        winding = turn;
    }
    Some(order)
}

pub(crate) fn pack(mesh: &TessellatedPath) -> Option<(Vec<PathVertex>, Vec<u32>)> {
    if !eligible(mesh) {
        return None;
    }
    let mut vertices = Vec::new();
    let mut mapping = vec![0; mesh.vertices.len()];
    for (index, v) in mesh.vertices.iter().enumerate() {
        if v.surface == PathSurface::Fill {
            mapping[index] = u32::try_from(vertices.len()).expect("bounded fill vertices");
            vertices.push(PathVertex {
                position: [v.position.x, v.position.y],
                target_position: [v.target_position.x, v.target_position.y],
                surface: pack_path_surface(v.surface, v.path_progress),
                polygon: [[[0.0; 2]; 2]; 3],
            });
        }
    }
    let mut indices = Vec::new();
    let triangles = mesh.indices.as_chunks::<3>().0;
    let mut cursor = 0;
    while cursor < triangles.len() {
        let triangle = triangles[cursor];
        if mesh.vertices[triangle[0] as usize].surface == PathSurface::Fill {
            indices.extend(triangle.map(|i| mapping[i as usize]));
            cursor += 1;
            continue;
        }
        let quad = triangles
            .get(cursor + 1)
            .filter(|t| mesh.vertices[t[0] as usize].surface == PathSurface::Stroke)
            .and_then(|&next| convex_pair(mesh, triangle, next));
        let corners = quad.unwrap_or([triangle[0], triangle[1], triangle[2], triangle[2]]);
        let points = corners.map(|i| {
            let p = mesh.vertices[i as usize].position;
            [[p.x, p.y]; 2]
        });
        let base = u32::try_from(vertices.len()).expect("bounded stroke vertices");
        for (corner, &source_index) in corners.iter().enumerate() {
            vertices.push(PathVertex {
                position: points[0][0],
                target_position: points[0][1],
                surface: pack_path_surface(
                    PathSurface::Stroke,
                    mesh.vertices[source_index as usize].path_progress,
                ) | POLYGON_COVERAGE_FLAG
                    | if quad.is_some() {
                        POLYGON_QUAD_FLAG | POLYGON_TIGHT_FLAG
                    } else {
                        0
                    }
                    | (u32::try_from(corner).expect("quad corner") << POLYGON_CORNER_SHIFT),
                polygon: [points[1], points[2], points[3]],
            });
        }
        indices.extend(TRIANGLE_QUAD_INDICES.map(|i| base + i));
        cursor += if quad.is_some() { 2 } else { 1 };
    }
    Some((vertices, indices))
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{StrokeCap, StrokeJoin, Vec2, VectorPath};

    #[test]
    fn stroke_packing_preserves_fill_and_original_reveal_triangles() {
        let path = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .line_to(Vec2::new(1.0, 0.0));
        let mesh = noon_geometry::tessellate_styled_with_fill(
            &path,
            0.04,
            StrokeJoin::Miter,
            StrokeCap::Butt,
            false,
        )
        .unwrap();
        let (vertices, indices) = pack(&mesh).unwrap();
        assert_eq!(vertices.len(), 4);
        assert_eq!(indices, TRIANGLE_QUAD_INDICES);
        for v in vertices {
            assert_ne!(v.surface & POLYGON_TIGHT_FLAG, 0);
            assert_eq!(v.surface & 1, 1);
        }
    }

    #[test]
    fn curves_round_styles_morphs_and_large_meshes_retain_ordinary_tessellation() {
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(1.0, 0.0));
        for (join, cap) in [
            (StrokeJoin::Round, StrokeCap::Butt),
            (StrokeJoin::Miter, StrokeCap::Round),
        ] {
            let mesh =
                noon_geometry::tessellate_styled_with_fill(&path, 0.04, join, cap, false).unwrap();
            assert!(pack(&mesh).is_none());
        }
        let curved = VectorPath::new().move_to(Vec2::ZERO).cubic_to(
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
        );
        let mesh = noon_geometry::tessellate_styled_with_fill(
            &curved,
            0.04,
            StrokeJoin::Miter,
            StrokeCap::Butt,
            false,
        )
        .unwrap();
        assert!(pack(&mesh).is_none());
        let mut mesh = noon_geometry::tessellate_styled_with_fill(
            &path,
            0.04,
            StrokeJoin::Miter,
            StrokeCap::Butt,
            false,
        )
        .unwrap();
        mesh.morphing = true;
        assert!(pack(&mesh).is_none());
        mesh.morphing = false;
        mesh.vertices.resize(129, mesh.vertices[0]);
        assert!(pack(&mesh).is_none());
    }

    #[test]
    fn covered_quad_payload_recovers_every_original_reveal_triangle() {
        let path = VectorPath::new()
            .move_to(Vec2::new(-1.0, -1.0))
            .line_to(Vec2::new(1.0, -1.0))
            .line_to(Vec2::new(1.0, 1.0))
            .line_to(Vec2::new(-1.0, 1.0))
            .close();
        let mesh = noon_geometry::tessellate_styled_with_fill(
            &path,
            0.04,
            StrokeJoin::Miter,
            StrokeCap::Butt,
            true,
        )
        .unwrap();
        let (vertices, indices) = pack(&mesh).unwrap();
        let payload = |v: PathVertex| {
            let point = v.local_corner(false);
            [
                point[0].to_bits(),
                point[1].to_bits(),
                v.surface & ((1 << 25) - 1),
            ]
        };
        let mut original = mesh
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let mut values = t.map(|i| {
                    let v = mesh.vertices[i as usize];
                    [
                        v.position.x.to_bits(),
                        v.position.y.to_bits(),
                        pack_path_surface(v.surface, v.path_progress),
                    ]
                });
                values.sort();
                values
            })
            .collect::<Vec<_>>();
        let mut recovered = indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let mut values = t.map(|i| payload(vertices[i as usize]));
                values.sort();
                values
            })
            .collect::<Vec<_>>();
        original.sort();
        recovered.sort();
        assert_eq!(original, recovered);
    }
}
