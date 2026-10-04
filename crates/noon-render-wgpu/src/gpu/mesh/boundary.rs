//! Immutable triangle-boundary topology, derived only when a mesh requests a stroke.
//! Shared edges (including a quad's triangulation diagonal) are not stroked.

use super::{SpatialPrepareError, Vertex};
use bytemuck::{Pod, Zeroable};
use noon_core::MeshResource;
use std::collections::BTreeMap;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct EdgeVertex {
    start: [f32; 3],
    end: [f32; 3],
    corner: [f32; 2],
}

pub(super) fn vertices(mesh: &MeshResource) -> Result<Vec<EdgeVertex>, SpatialPrepareError> {
    let positions = super::lower_vertices(mesh)?;
    let mut edges = BTreeMap::<(u32, u32), u32>::new();
    for triangle in mesh.indices().as_chunks::<3>().0 {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let edge = if a < b { (a, b) } else { (b, a) };
            *edges.entry(edge).or_default() += 1;
        }
    }
    let mut result = Vec::new();
    for ((a, b), count) in edges {
        if count != 1 || a == b {
            continue;
        }
        let Vertex {
            position: start, ..
        } = positions[a as usize];
        let Vertex { position: end, .. } = positions[b as usize];
        if start == end {
            continue;
        }
        for corner in [
            [0., -1.],
            [1., -1.],
            [1., 1.],
            [0., -1.],
            [1., 1.],
            [0., 1.],
        ] {
            result.push(EdgeVertex { start, end, corner });
        }
    }
    Ok(result)
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
        let degenerate =
            MeshResource::new(vec![SemanticVec3::ZERO; 3], None, vec![0, 1, 2]).unwrap();
        assert!(vertices(&degenerate).unwrap().is_empty());
    }
}
