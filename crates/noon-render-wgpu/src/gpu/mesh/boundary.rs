//! Immutable face geometry and exterior strokes derived during resource staging.
//! Camera/light edits reuse their GPU buffers. Shared triangle edges are not stroked.

use super::{SpatialPrepareError, Vertex};
use bytemuck::{Pod, Zeroable};
use noon_core::{MeshResource, SemanticVec3};
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
}

/// Cairo face fans share the retained border subdivision.
pub(super) fn fill_geometry(mesh: &MeshResource) -> Result<FillGeometry, SpatialPrepareError> {
    let corners = super::lower_vertices(mesh)?;
    let Some(appearance) = mesh.cairo_appearance() else {
        return Ok(FillGeometry {
            vertices: corners,
            indices: mesh.indices().to_vec(),
        });
    };
    let Some(controls) = appearance.boundary_controls else {
        // Non-sampled Cairo faces retain their authored triangle topology.
        return Ok(FillGeometry {
            vertices: corners,
            indices: mesh.indices().to_vec(),
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
    let count = vertices.len() as u32;
    let mut indices = Vec::with_capacity(48);
    for point in 1..count {
        indices.extend([0, point, if point + 1 == count { 1 } else { point + 1 }]);
    }
    Ok(FillGeometry { vertices, indices })
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

fn face_center(points: &[SemanticVec3]) -> SemanticVec3 {
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
