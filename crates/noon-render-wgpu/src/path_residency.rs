//! Installed immutable path meshes occupy a fixed GPU geometry prefix.
use super::*;

/// A typed renderer specialization to prepare before playback. Exact duplicate
/// geometry/style/transform mesh keys share a resident range.
#[derive(Clone, Copy, Debug)]
pub struct PathMeshPreload<'a> {
    pub geometry: &'a GeometryRef,
    pub style: Style,
    pub transform: Transform2D,
}

/// Successful installation and queued GPU writes for one resident resource set.
#[derive(Debug)]
pub struct PathMeshPreloadStats {
    pub geometry: RenderStats,
    pub upload: UploadStats,
}

#[derive(Debug)]
pub enum PathMeshPreloadError {
    Geometry(noon_geometry::GeometryError),
    Upload(PathPreloadUploadError),
}

impl std::fmt::Display for PathMeshPreloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Geometry(error) => std::fmt::Display::fmt(error, f),
            Self::Upload(error) => std::fmt::Display::fmt(error, f),
        }
    }
}
impl std::error::Error for PathMeshPreloadError {}
impl From<noon_geometry::GeometryError> for PathMeshPreloadError {
    fn from(error: noon_geometry::GeometryError) -> Self {
        Self::Geometry(error)
    }
}
impl From<PathPreloadUploadError> for PathMeshPreloadError {
    fn from(error: PathPreloadUploadError) -> Self {
        Self::Upload(error)
    }
}

#[derive(Clone, Debug)]
pub(super) struct ResidentPathRanges {
    pub vertices: Range<u32>,
    pub indices: Range<u32>,
}

impl FramePreparer {
    pub(crate) fn preload_paths(
        &mut self,
        requests: &[PathMeshPreload<'_>],
    ) -> Result<(), noon_geometry::GeometryError> {
        debug_assert!(self.individual_path_draws && self.path_mesh_cache.is_empty());
        for request in requests {
            validate_request(request)?;
            let GeometryRef::VectorPath(path) = request.geometry else {
                return Err(noon_geometry::GeometryError::Tessellation(
                    "path preload requires vector geometry".into(),
                ));
            };
            let (index, _) = self.cache_path_mesh(path, request.style, request.transform)?;
            if self.path_mesh_cache[index].resident.is_some() {
                continue;
            }
            checked_packed_end(
                self.path_vertices.len(),
                self.path_mesh_cache[index].mesh.vertices.len(),
            )?;
            checked_packed_end(
                self.path_indices.len(),
                self.path_mesh_cache[index].mesh.indices.len(),
            )?;
            let ranges = append_mesh(
                &self.path_mesh_cache[index].mesh,
                &mut self.path_vertices,
                &mut self.path_indices,
            );
            self.path_mesh_cache[index].resident = Some(ranges);
        }
        self.resident_vertex_count = self.path_vertices.len();
        self.resident_index_count = self.path_indices.len();
        self.path_geometry_dirty =
            self.resident_vertex_count != 0 || self.resident_index_count != 0;
        if self.resident_vertex_count != 0 {
            self.path_vertex_dirty_ranges
                .push(0..self.resident_vertex_count);
        }
        if self.resident_index_count != 0 {
            self.path_index_dirty_ranges
                .push(0..self.resident_index_count);
        }
        Ok(())
    }

    pub(crate) fn preloaded_frame(&self) -> PreparedFrame<'_> {
        self.prepared_frame(
            0.0,
            0,
            0,
            self.path_mesh_cache.len(),
            self.resident_vertex_count,
            self.resident_index_count,
            0,
            0,
            0,
        )
    }

    pub(super) fn pack_resident_path_groups(
        &mut self,
        groups: Vec<PathGroup>,
    ) -> (Vec<usize>, usize, usize) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut offsets = Vec::with_capacity(groups.len());
        for group in groups {
            let start = self.paths.len();
            offsets.push(start);
            self.path_ids.extend(group.ids);
            self.paths.extend(group.instances);
            let entry = &self.path_mesh_cache[group.cache_index];
            let ranges = entry.resident.clone().unwrap_or_else(|| {
                append_mesh_at(
                    &entry.mesh,
                    &mut vertices,
                    &mut indices,
                    self.resident_vertex_count,
                    self.resident_index_count,
                )
            });
            self.path_batch_vertex_ranges.push(ranges.vertices);
            self.path_batches.push(PathBatch {
                index_range: ranges.indices,
                instance_range: u32::try_from(start).expect("path instance limit")
                    ..u32::try_from(self.paths.len()).expect("path instance limit"),
            });
            self.path_batch_cache_indices.push(group.cache_index);
        }
        // Only the disposable suffix changes across phases. A resident mesh is
        // never copied here, even when its batch becomes visible for the first time.
        let changed = self.path_vertices[self.resident_vertex_count..] != vertices
            || self.path_indices[self.resident_index_count..] != indices;
        let repacked = if changed {
            (vertices.len(), indices.len())
        } else {
            (0, 0)
        };
        if changed {
            self.path_vertices.truncate(self.resident_vertex_count);
            self.path_indices.truncate(self.resident_index_count);
            self.path_vertices.extend(vertices);
            self.path_indices.extend(indices);
            if self.path_vertices.len() > self.resident_vertex_count {
                self.path_vertex_dirty_ranges
                    .push(self.resident_vertex_count..self.path_vertices.len());
            }
            if self.path_indices.len() > self.resident_index_count {
                self.path_index_dirty_ranges
                    .push(self.resident_index_count..self.path_indices.len());
            }
        }
        self.path_geometry_dirty = changed;
        self.packed_path_mesh_cache_generation = self.path_mesh_cache_generation;
        (offsets, repacked.0, repacked.1)
    }
}

fn validate_request(request: &PathMeshPreload<'_>) -> Result<(), noon_geometry::GeometryError> {
    let transform = request.transform;
    let style = request.style;
    let finite_colors = [style.fill, style.stroke]
        .into_iter()
        .flatten()
        .all(|color| {
            [color.red, color.green, color.blue, color.alpha]
                .into_iter()
                .all(f32::is_finite)
        });
    if !request.geometry.is_finite()
        || !finite_colors
        || ![
            transform.translation.x,
            transform.translation.y,
            transform.scale.x,
            transform.scale.y,
            transform.rotation,
            style.opacity,
        ]
        .into_iter()
        .all(f32::is_finite)
    {
        return Err(noon_geometry::GeometryError::NonFinitePoint);
    }
    if !style.stroke_width.is_finite() || style.stroke_width < 0.0 {
        return Err(noon_geometry::GeometryError::InvalidStrokeWidth(
            style.stroke_width,
        ));
    }
    Ok(())
}

fn append_mesh(
    mesh: &TessellatedPath,
    vertices: &mut Vec<PathVertex>,
    indices: &mut Vec<u32>,
) -> ResidentPathRanges {
    append_mesh_at(mesh, vertices, indices, 0, 0)
}

fn append_mesh_at(
    mesh: &TessellatedPath,
    vertices: &mut Vec<PathVertex>,
    indices: &mut Vec<u32>,
    vertex_base: usize,
    index_base: usize,
) -> ResidentPathRanges {
    let vertex_start = u32::try_from(vertex_base + vertices.len()).expect("path vertex limit");
    let index_start = u32::try_from(index_base + indices.len()).expect("path index limit");
    vertices.extend(mesh.vertices.iter().map(|vertex| PathVertex {
        position: [vertex.position.x, vertex.position.y],
        target_position: [vertex.target_position.x, vertex.target_position.y],
        surface: pack_path_surface(vertex.surface, vertex.path_progress),
    }));
    indices.extend(
        mesh.indices
            .iter()
            .map(|index| index.checked_add(vertex_start).expect("path index limit")),
    );
    ResidentPathRanges {
        vertices: vertex_start
            ..u32::try_from(vertex_base + vertices.len()).expect("path vertex limit"),
        indices: index_start..u32::try_from(index_base + indices.len()).expect("path index limit"),
    }
}

fn checked_packed_end(start: usize, length: usize) -> Result<u32, noon_geometry::GeometryError> {
    start
        .checked_add(length)
        .and_then(|end| u32::try_from(end).ok())
        .ok_or_else(|| {
            noon_geometry::GeometryError::Tessellation(
                "preloaded geometry exceeds renderer address limits".into(),
            )
        })
}

#[cfg(test)]
mod tests {
    #[test]
    fn oversized_preload_address_is_rejected_without_allocation() {
        assert!(super::checked_packed_end(u32::MAX as usize, 1).is_err());
        assert!(super::checked_packed_end(usize::MAX, 1).is_err());
        assert_eq!(super::checked_packed_end(0, 1).unwrap(), 1);
    }
}
