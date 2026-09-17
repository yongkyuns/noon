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

/// A validated/tessellated resident suffix that has not displaced the current
/// disposable frame geometry yet. This lets the host check GPU allocation limits
/// before committing the new immutable prefix.
#[derive(Debug)]
pub(crate) struct PathMeshAppendPlan {
    cache_indices: Vec<usize>,
    geometry_cache_misses: usize,
    next_vertex_count: usize,
    next_index_count: usize,
}

impl PathMeshAppendPlan {
    pub(crate) fn is_empty(&self) -> bool {
        self.cache_indices.is_empty()
    }

    pub(crate) fn next_vertex_count(&self) -> usize {
        self.next_vertex_count
    }

    pub(crate) fn next_index_count(&self) -> usize {
        self.next_index_count
    }
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
            let (index, _) = match self.cache_path_mesh(path, request.style, request.transform) {
                Ok(cached) => cached,
                Err(_) if sampled_morph::prepare_sampled_path(path, request.style).is_ok() => {
                    continue
                }
                Err(error) => return Err(error),
            };
            if self.path_mesh_cache[index].resident.is_some() {
                continue;
            }
            checked_packed_end(
                self.path_vertices.len(),
                packed_path_vertex_count(&self.path_mesh_cache[index].mesh),
            )?;
            checked_packed_end(
                self.path_indices.len(),
                packed_path_index_count(&self.path_mesh_cache[index].mesh),
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

    /// Tessellate and validate only the requested incremental resident suffix.
    /// Existing resident ranges and current-frame packed geometry remain untouched
    /// until `commit_path_mesh_append` is called after device-limit validation.
    pub(crate) fn prepare_path_mesh_append(
        &mut self,
        requests: &[PathMeshPreload<'_>],
    ) -> Result<PathMeshAppendPlan, noon_geometry::GeometryError> {
        debug_assert!(self.individual_path_draws);
        let cache_start = self.path_mesh_cache.len();
        let mut cache_indices = Vec::new();
        let mut geometry_cache_misses = 0usize;

        for request in requests {
            let result = (|| {
                validate_request(request)?;
                let GeometryRef::VectorPath(path) = request.geometry else {
                    return Err(noon_geometry::GeometryError::Tessellation(
                        "path preload requires vector geometry".into(),
                    ));
                };
                let (index, cache_miss) =
                    match self.cache_path_mesh(path, request.style, request.transform) {
                        Ok(cached) => cached,
                        Err(_)
                            if sampled_morph::prepare_sampled_path(path, request.style).is_ok() =>
                        {
                            return Ok(())
                        }
                        Err(error) => return Err(error),
                    };
                geometry_cache_misses += usize::from(cache_miss);
                if self.path_mesh_cache[index].resident.is_none() && !cache_indices.contains(&index)
                {
                    cache_indices.push(index);
                }
                Ok::<_, noon_geometry::GeometryError>(())
            })();
            if let Err(error) = result {
                self.rollback_cached_path_suffix(cache_start);
                return Err(error);
            }
        }

        let mut next_vertex_count = self.resident_vertex_count;
        let mut next_index_count = self.resident_index_count;
        for &index in &cache_indices {
            let mesh = &self.path_mesh_cache[index].mesh;
            match checked_packed_end(next_vertex_count, packed_path_vertex_count(mesh)) {
                Ok(end) => next_vertex_count = end as usize,
                Err(error) => {
                    self.rollback_cached_path_suffix(cache_start);
                    return Err(error);
                }
            }
            match checked_packed_end(next_index_count, packed_path_index_count(mesh)) {
                Ok(end) => next_index_count = end as usize,
                Err(error) => {
                    self.rollback_cached_path_suffix(cache_start);
                    return Err(error);
                }
            }
        }

        Ok(PathMeshAppendPlan {
            cache_indices,
            geometry_cache_misses,
            next_vertex_count,
            next_index_count,
        })
    }

    /// Commit one already validated resident suffix. The previous frame's packed
    /// path suffix is disposable, so dropping it does not copy old geometry. The
    /// next prepare rebuilds that suffix after the immutable prefix has grown.
    pub(crate) fn commit_path_mesh_append(&mut self, plan: PathMeshAppendPlan) -> RenderStats {
        if plan.cache_indices.is_empty() {
            return RenderStats {
                geometry_cache_misses: plan.geometry_cache_misses,
                ..RenderStats::default()
            };
        }

        let vertex_start = self.resident_vertex_count;
        let index_start = self.resident_index_count;
        self.path_vertices.truncate(vertex_start);
        self.path_indices.truncate(index_start);
        self.path_vertex_free_ranges.clear();
        self.path_index_free_ranges.clear();
        self.path_vertex_dirty_ranges.clear();
        self.path_index_dirty_ranges.clear();

        for index in plan.cache_indices {
            debug_assert!(self.path_mesh_cache[index].resident.is_none());
            let ranges = append_mesh(
                &self.path_mesh_cache[index].mesh,
                &mut self.path_vertices,
                &mut self.path_indices,
            );
            self.path_mesh_cache[index].resident = Some(ranges);
        }

        self.resident_vertex_count = self.path_vertices.len();
        self.resident_index_count = self.path_indices.len();
        debug_assert_eq!(self.resident_vertex_count, plan.next_vertex_count);
        debug_assert_eq!(self.resident_index_count, plan.next_index_count);
        self.path_vertex_dirty_ranges
            .push(vertex_start..self.resident_vertex_count);
        self.path_index_dirty_ranges
            .push(index_start..self.resident_index_count);
        self.path_geometry_dirty = true;
        // Existing draw descriptors refer to the disposable suffix we just dropped.
        // They are never submitted again: the next retained preparation rebuilds them.
        self.initialized = false;

        RenderStats {
            geometry_cache_misses: plan.geometry_cache_misses,
            path_vertices_repacked: self.resident_vertex_count - vertex_start,
            path_indices_repacked: self.resident_index_count - index_start,
            ..RenderStats::default()
        }
    }

    /// Empty-draw upload view over the cumulative resident prefix. It intentionally
    /// hides stale per-frame draw descriptors while retaining the exact dirty suffix,
    /// so the existing preload uploader can write only new resident ranges.
    pub(crate) fn resident_upload_frame(&self, stats: RenderStats) -> PreparedFrame<'_> {
        PreparedFrame {
            time: 0.0,
            circle_ids: &[],
            circles: &[],
            rectangle_ids: &[],
            rectangles: &[],
            line_ids: &[],
            lines: &[],
            path_ids: &[],
            paths: &[],
            path_vertices: &self.path_vertices,
            path_indices: &self.path_indices,
            path_batches: &[],
            mega_path_indices: &[],
            mega_path_vertex_instances: &[],
            mega_path_batches: &[],
            render_batches: &[],
            render_chunks: &[],
            render_chunks_active: false,
            unsupported: &[],
            circle_dirty_ranges: &[],
            rectangle_dirty_ranges: &[],
            line_dirty_ranges: &[],
            path_dirty_ranges: &[],
            path_vertex_dirty_ranges: &self.path_vertex_dirty_ranges,
            path_index_dirty_ranges: &self.path_index_dirty_ranges,
            mega_path_instance_dirty_ranges: &[],
            mega_path_index_dirty_ranges: &[],
            mega_path_index_dirty: false,
            path_geometry_dirty: self.path_geometry_dirty,
            stats,
            slots: &[],
            slot_presences: &[],
            complete_submission: false,
        }
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

    fn rollback_cached_path_suffix(&mut self, cache_start: usize) {
        self.path_mesh_cache.truncate(cache_start);
        self.path_mesh_lookup.retain(|_, indices| {
            indices.retain(|&index| index < cache_start);
            !indices.is_empty()
        });
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
    let (packed_vertices, local_indices) = pack_path_mesh(mesh);
    vertices.extend_from_slice(&packed_vertices);
    indices.extend(
        local_indices
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
