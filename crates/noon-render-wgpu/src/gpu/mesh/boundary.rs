//! Immutable face geometry and exterior strokes derived during resource staging.
//! Camera/light edits reuse their GPU buffers. Shared triangle edges are not stroked.

use super::{SpatialPrepareError, Vertex};
use bytemuck::{Pod, Zeroable};
use noon_core::{MeshResource, SemanticVec3, Vec2};
use std::collections::BTreeMap;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct EdgeVertex {
    start: [f32; 3],
    end: [f32; 3],
    corner: [f32; 2],
    face_point: [f32; 3],
}

pub(super) fn vertices(mesh: &MeshResource) -> Result<Vec<EdgeVertex>, SpatialPrepareError> {
    let positions = super::lower_vertices(mesh)?;
    let face_center = mesh
        .cairo_appearance()
        .and_then(|appearance| appearance.boundary_controls)
        .is_some()
        .then(|| lower_point(face_center(mesh.positions())))
        .transpose()?;
    let edges = topology(mesh);
    let mut result = Vec::new();
    for ((a, b), (count, opposite)) in edges {
        if count != 1 || a == b {
            continue;
        }
        let Vertex {
            position: start, ..
        } = positions[a as usize];
        let Vertex { position: end, .. } = positions[b as usize];
        let controls = mesh.cairo_appearance().and_then(|appearance| {
            appearance.boundary_controls.map(|controls| match (a, b) {
                (0, 1) => controls[0],
                (1, 2) => controls[1],
                (2, 3) => controls[2],
                (0, 3) => [controls[3][1], controls[3][0]],
                _ => unreachable!("validated Cairo quad boundary"),
            })
        });
        let from = mesh.positions()[a as usize];
        let to = mesh.positions()[b as usize];
        if start == end
            && controls.is_none_or(|controls| controls.iter().all(|point| *point == from))
        {
            continue;
        }
        // Four immutable chords per curved edge bound staging and upload work.
        // Straight surface edges retain the original single segment. Camera
        // movement only projects this retained data, never resamples Python.
        let segments = segment_count(from, controls, to);
        let mut start = start;
        for segment in 0..segments {
            let end = if segment + 1 == segments {
                end
            } else {
                let controls = controls.expect("curved Cairo edge");
                lower_point(cubic(
                    from,
                    controls,
                    to,
                    f64::from(segment + 1) / f64::from(segments),
                ))?
            };
            for corner in [
                [0., -1.],
                [1., -1.],
                [1., 1.],
                [0., -1.],
                [1., 1.],
                [0., 1.],
            ] {
                result.push(EdgeVertex {
                    start,
                    end,
                    corner,
                    face_point: face_center.unwrap_or(positions[opposite as usize].position),
                });
            }
            start = end;
        }
    }
    Ok(result)
}

pub(super) struct FillGeometry {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub planar_proxy: Option<[u32; 2]>,
}

/// Cairo face fans share the retained border subdivision.
pub(super) fn fill_geometry(mesh: &MeshResource) -> Result<FillGeometry, SpatialPrepareError> {
    let corners = super::lower_vertices(mesh)?;
    let Some(appearance) = mesh.cairo_appearance() else {
        return Ok(FillGeometry {
            vertices: corners,
            indices: mesh.indices().to_vec(),
            planar_proxy: None,
        });
    };
    let Some(controls) = appearance.boundary_controls else {
        // Non-sampled Cairo faces retain their authored triangle topology.
        return Ok(FillGeometry {
            vertices: corners,
            indices: mesh.indices().to_vec(),
            planar_proxy: None,
        });
    };
    let points = mesh.positions();
    let mut vertices = Vec::with_capacity(17);
    vertices.push(Vertex {
        position: lower_point(face_center(points))?,
        normal: std::array::from_fn(|axis| {
            corners
                .iter()
                .map(|vertex| vertex.normal[axis] / corners.len() as f32)
                .sum()
        }),
    });
    for from in 0..4 {
        let to = (from + 1) % 4;
        let controls = controls[from];
        let segments = segment_count(points[from], Some(controls), points[to]);
        for segment in 0..segments {
            let t = f64::from(segment) / f64::from(segments);
            let point = cubic(points[from], controls, points[to], t);
            vertices.push(Vertex {
                position: lower_point(point)?,
                normal: std::array::from_fn(|axis| {
                    corners[from].normal[axis] * (1. - t as f32)
                        + corners[to].normal[axis] * t as f32
                }),
            });
        }
    }
    let (center, planar_proxy) = planar_fill_support(&vertices[1..], vertices[0].position);
    vertices[0].position = center;
    let count = vertices.len() as u32;
    let mut indices = Vec::with_capacity(48);
    for point in 1..count {
        indices.extend([0, point, if point + 1 == count { 1 } else { point + 1 }]);
    }
    Ok(FillGeometry {
        vertices,
        indices,
        planar_proxy,
    })
}

/// Select depth-plane support for a planar ring with an invalid fan anchor.
/// The four-chord perimeter remains authoritative for the fill and border.
fn planar_fill_support(vertices: &[Vertex], original: [f32; 3]) -> ([f32; 3], Option<[u32; 2]>) {
    let origin = vertices[0].position.map(f64::from);
    let points: Vec<[f64; 3]> = vertices
        .iter()
        .map(|v| std::array::from_fn(|axis| f64::from(v.position[axis]) - origin[axis]))
        .collect();
    let mut normal = [0.0; 3];
    for (a, b) in points.iter().zip(points.iter().cycle().skip(1)) {
        for (axis, component) in normal.iter_mut().enumerate() {
            let u = (axis + 1) % 3;
            let v = (axis + 2) % 3;
            *component += a[u] * b[v] - a[v] * b[u];
        }
    }
    let axis = (0..3)
        .max_by(|&a, &b| normal[a].abs().total_cmp(&normal[b].abs()))
        .expect("three coordinate axes");
    let area = normal[axis];
    if area == 0.0 {
        return (original, None);
    }
    let scale = points.iter().flatten().fold(1.0_f64, |s, v| s.max(v.abs()));
    if points.iter().any(|p| {
        let distance: f64 = p.iter().zip(normal).map(|(p, n)| p * n).sum();
        distance.abs() > area.abs() * scale * 1e-6
    }) {
        return (original, None);
    }
    let u = (axis + 1) % 3;
    let v = (axis + 2) % 3;
    let cross = |a: &[f64; 3], b: &[f64; 3]| a[u] * b[v] - a[v] * b[u];
    let in_kernel = |center: [f32; 3]| {
        let center: [f64; 3] = std::array::from_fn(|axis| f64::from(center[axis]) - origin[axis]);
        points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .all(|(a, b)| {
                let edge = std::array::from_fn(|axis| b[axis] - a[axis]);
                let offset = std::array::from_fn(|axis| center[axis] - a[axis]);
                cross(&edge, &offset) * area.signum() >= 0.0
            })
    };
    if in_kernel(original) {
        return (original, None);
    }
    let contour_scale = points
        .iter()
        .map(|point| point[u].abs().max(point[v].abs()))
        .fold(0.0_f64, f64::max);
    let contour: Vec<_> = points
        .iter()
        .map(|point| {
            Vec2::new(
                (point[u] / contour_scale) as f32,
                (point[v] / contour_scale) as f32,
            )
        })
        .collect();
    if !noon_geometry::polygon_is_simple(&contour) {
        return (original, None);
    }
    let mut centroid = [0.0; 3];
    for (a, b) in points.iter().zip(points.iter().cycle().skip(1)) {
        let weight = cross(a, b) / (3.0 * area);
        for axis in 0..3 {
            centroid[axis] += (a[axis] + b[axis]) * weight;
        }
    }
    let candidate = std::array::from_fn(|axis| (origin[axis] + centroid[axis]) as f32);
    let center = if candidate.iter().all(|value| value.is_finite()) && in_kernel(candidate) {
        // Keep the better fan for views where the planar proxy is clipped.
        candidate
    } else {
        original
    };
    // Pick a wide basis from the plane through perimeter[0]. Consecutive
    // perimeter chords can be nearly collinear or even collapse together.
    let length_squared = |point: [f64; 3]| point[u] * point[u] + point[v] * point[v];
    let second = (1..points.len())
        .max_by(|&a, &b| length_squared(points[a]).total_cmp(&length_squared(points[b])))
        .expect("a nonzero-area ring has multiple points");
    let third = (1..points.len())
        .max_by(|&a, &b| {
            cross(&points[second], &points[a])
                .abs()
                .total_cmp(&cross(&points[second], &points[b]).abs())
        })
        .expect("a nonzero-area ring has multiple points");
    (center, Some([second as u32, third as u32]))
}

fn topology(mesh: &MeshResource) -> BTreeMap<(u32, u32), (u32, u32)> {
    let mut edges = BTreeMap::new();
    for triangle in mesh.indices().as_chunks::<3>().0 {
        for (a, b, opposite) in [
            (triangle[0], triangle[1], triangle[2]),
            (triangle[1], triangle[2], triangle[0]),
            (triangle[2], triangle[0], triangle[1]),
        ] {
            let edge = if a < b { (a, b) } else { (b, a) };
            edges.entry(edge).or_insert((0, opposite)).0 += 1;
        }
    }
    edges
}

fn segment_count(from: SemanticVec3, controls: Option<[SemanticVec3; 2]>, to: SemanticVec3) -> u32 {
    controls.map_or(1, |controls| {
        let linear = [
            interpolate(from, to, 1. / 3.),
            interpolate(from, to, 2. / 3.),
        ];
        let scale = (from.x - to.x)
            .abs()
            .max((from.y - to.y).abs())
            .max((from.z - to.z).abs())
            .max(1.0);
        if controls.into_iter().zip(linear).all(|(a, b)| {
            (a.x - b.x)
                .abs()
                .max((a.y - b.y).abs())
                .max((a.z - b.z).abs())
                <= scale * 1e-7
        }) {
            1
        } else {
            4
        }
    })
}

pub(super) fn face_center(points: &[SemanticVec3]) -> SemanticVec3 {
    points.iter().fold(SemanticVec3::ZERO, |center, point| {
        SemanticVec3::new(
            center.x + point.x / points.len() as f64,
            center.y + point.y / points.len() as f64,
            center.z + point.z / points.len() as f64,
        )
    })
}

fn interpolate(a: SemanticVec3, b: SemanticVec3, t: f64) -> SemanticVec3 {
    SemanticVec3::new(
        a.x * (1. - t) + b.x * t,
        a.y * (1. - t) + b.y * t,
        a.z * (1. - t) + b.z * t,
    )
}

fn cubic(a: SemanticVec3, controls: [SemanticVec3; 2], b: SemanticVec3, t: f64) -> SemanticVec3 {
    let left = interpolate(a, controls[0], t);
    let middle = interpolate(controls[0], controls[1], t);
    let right = interpolate(controls[1], b, t);
    interpolate(
        interpolate(left, middle, t),
        interpolate(middle, right, t),
        t,
    )
}

fn lower_point(point: SemanticVec3) -> Result<[f32; 3], SpatialPrepareError> {
    let point = [point.x as f32, point.y as f32, point.z as f32];
    if point.iter().all(|value| value.is_finite()) {
        Ok(point)
    } else {
        Err(SpatialPrepareError::UnrepresentableVertex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::SemanticVec3;

    #[test]
    fn planar_fill_support_checks_winding_and_rejects_nonplanar_or_crossing_rings() {
        let ring = [
            [-0.5, -0.5, 0.0],
            [-0.25, -0.21875, 0.0],
            [0.0, -0.125, 0.0],
            [0.25, -0.21875, 0.0],
            [0.5, -0.5, 0.0],
            [0.5, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
        ];
        for axis in 0..3 {
            for reversed in [false, true] {
                let original = [16.0, -8.0, 4.0];
                let mut vertices: Vec<_> = ring
                    .iter()
                    .map(|p| Vertex {
                        position: std::array::from_fn(|i| original[i] + p[(i + axis) % 3]),
                        normal: [0.0; 3],
                    })
                    .collect();
                if reversed {
                    vertices.reverse();
                }
                let (center, planar_proxy) = planar_fill_support(&vertices, original);
                assert!(planar_proxy.is_some());
                let shifted_axis = (4 - axis) % 3;
                assert!((center[shifted_axis] - original[shifted_axis] - 0.10905612).abs() < 1e-6);
                assert_eq!(planar_fill_support(&vertices, center), (center, None));
                vertices[0].position[(5 - axis) % 3] += 0.05;
                assert_eq!(planar_fill_support(&vertices, original), (original, None));
            }
        }
        let empty_kernel: Vec<_> = [
            [-0.5, -0.5, 0.0],
            [0.5, -0.5, 0.0],
            [0.5, 0.5, 0.0],
            [0.25, 0.5, 0.0],
            [0.25, 0.0, 0.0],
            [-0.25, 0.0, 0.0],
            [-0.25, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
        ]
        .into_iter()
        .map(|position| Vertex {
            position,
            normal: [0.0; 3],
        })
        .collect();
        let (center, plane) = planar_fill_support(&empty_kernel, [0.0, -0.25, 0.0]);
        assert_eq!(center, [0.0, -0.25, 0.0]);
        let [second, third] = plane.unwrap().map(|index| index as usize);
        assert_ne!(second, third);
        assert_ne!(
            empty_kernel[second].position[1],
            empty_kernel[0].position[1]
        );
        let crossing =
            [[0., 0., 0.], [2., 2., 0.], [0., 2., 0.], [3., 0., 0.]].map(|position| Vertex {
                position,
                normal: [0.; 3],
            });
        assert_eq!(
            planar_fill_support(&crossing, [1., 1., 0.]),
            ([1., 1., 0.], None)
        );
        let collapsed = [Vertex {
            position: [0.0; 3],
            normal: [0.0; 3],
        }; 4];
        assert_eq!(planar_fill_support(&collapsed, [0.0; 3]), ([0.0; 3], None));
    }

    #[test]
    fn quad_boundary_omits_the_shared_diagonal_and_degenerate_edges() {
        let quad = MeshResource::new(
            vec![
                SemanticVec3::ZERO,
                SemanticVec3::new(1., 0., 0.),
                SemanticVec3::new(1., 1., 0.),
                SemanticVec3::new(0., 1., 0.),
            ],
            None,
            vec![0, 1, 3, 1, 2, 3],
        )
        .unwrap();
        assert_eq!(vertices(&quad).unwrap().len(), 4 * 6);
        let fill = fill_geometry(&quad).unwrap();
        assert_eq!(fill.vertices.len(), 4);
        assert_eq!(fill.indices, quad.indices());
        let degenerate =
            MeshResource::new(vec![SemanticVec3::ZERO; 3], None, vec![0, 1, 2]).unwrap();
        assert!(vertices(&degenerate).unwrap().is_empty());
    }

    #[test]
    fn cairo_controls_stage_bounded_curves_without_subdividing_straight_edges() {
        let points = [
            SemanticVec3::ZERO,
            SemanticVec3::new(1., 0., 0.),
            SemanticVec3::new(1., 1., 0.),
            SemanticVec3::new(0., 1., 0.),
        ];
        let quad = MeshResource::new(points.to_vec(), None, vec![0, 1, 3, 1, 2, 3]).unwrap();
        let mut controls = std::array::from_fn(|index| {
            let a = points[index];
            let b = points[(index + 1) % 4];
            [interpolate(a, b, 1. / 3.), interpolate(a, b, 2. / 3.)]
        });
        let appearance = |controls| noon_core::CairoSurfaceAppearance {
            p0: points[0],
            p6: points[2],
            span_p3_p0: SemanticVec3::new(1., 0., 0.),
            span_p12_p0: SemanticVec3::new(0., 1., 0.),
            span_p9_p6: SemanticVec3::new(-1., 0., 0.),
            span_p3_p6: SemanticVec3::new(0., -1., 0.),
            boundary_controls: Some(controls),
        };
        let mut plain_appearance = appearance(controls);
        plain_appearance.boundary_controls = None;
        let plain_cairo = quad
            .clone()
            .with_cairo_appearance(plain_appearance)
            .unwrap();
        let fill = fill_geometry(&plain_cairo).unwrap();
        assert_eq!(fill.vertices.len(), 4);
        assert_eq!(fill.indices, quad.indices());
        assert_eq!(
            vertices(
                &quad
                    .clone()
                    .with_cairo_appearance(appearance(controls))
                    .unwrap()
            )
            .unwrap()
            .len(),
            4 * 6
        );
        controls[0][0].y = -1.;
        controls[0][1].y = -1.;
        let curved = quad.with_cairo_appearance(appearance(controls)).unwrap();
        let edges = vertices(&curved).unwrap();
        assert_eq!(edges.len(), (4 + 3) * 6);
        assert_eq!(edges[6].start, [0.25, -0.5625, 0.]);
        assert_eq!(edges[6].end, [0.5, -0.75, 0.]);
        assert_eq!(edges[6].face_point, [0.5, 0.5, 0.]);
        assert!(edges.len() <= 4 * 4 * 6);
        let fill = fill_geometry(&curved).unwrap();
        assert_eq!(fill.vertices.len(), 8);
        assert_eq!(fill.vertices[3].position, edges[6].end);
        assert_eq!(fill.indices.len(), 7 * 3);
        assert!(fill.vertices.len() <= 17 && fill.indices.len() <= 48);
        let collapsed =
            MeshResource::new(vec![SemanticVec3::ZERO; 4], None, vec![0, 1, 3, 1, 2, 3]).unwrap();
        let mut loop_controls = [[SemanticVec3::ZERO; 2]; 4];
        loop_controls[0] = [SemanticVec3::new(1., 0., 0.), SemanticVec3::new(0., 1., 0.)];
        let curved_loop = collapsed
            .with_cairo_appearance(appearance(loop_controls))
            .unwrap();
        let edges = vertices(&curved_loop).unwrap();
        assert_eq!(
            edges.len(),
            4 * 6,
            "coincident anchors can still bound a visible curve"
        );
        assert!(edges.iter().any(|edge| edge.start != edge.end));
    }
}
