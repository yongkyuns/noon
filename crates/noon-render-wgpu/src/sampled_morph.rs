//! Renderer-local realization for filled paths without a stable fan triangulation.
//!
//! Safe fans and stroke-only morphs keep the immutable GPU endpoint fast path.
//! A complex fill keeps its immutable endpoint resource too, but owns one reusable
//! sampled mesh per presentation row. Progress does not create cache entries,
//! semantic nodes, execution slots, or cross-worker geometry resources.
use super::*;
use noon_geometry::{GeometryError, PreparedPathInterpolation};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum SampledMeshOwner {
    Stable(ObjectId),
    // A renderer cache key, never a semantic or stable execution identity.
    Derived { anchor: u32, occurrence: u32 },
}

#[derive(Clone, Debug)]
pub(super) struct SampledPathMesh {
    pub owner: SampledMeshOwner,
    pub interpolation: PreparedPathInterpolation,
    pub progress_bits: u32,
}

pub(super) fn prepare_sampled_path(
    path: &VectorPath,
    style: Style,
) -> Result<PreparedPathInterpolation, GeometryError> {
    let target = path.morph_target().filter(|_| style.fill.is_some())
        .ok_or_else(|| GeometryError::Tessellation("not a filled path morph".into()))?;
    PreparedPathInterpolation::new(path, target)
        .map_err(|error| GeometryError::Tessellation(error.to_string()))
}

pub(super) fn tessellate_path_at_progress(
    path: &VectorPath,
    style: Style,
    transform: Transform2D,
    progress: f32,
) -> Result<TessellatedPath, GeometryError> {
    match tessellate_path_mesh(path, style, transform) {
        Ok(mesh) => Ok(mesh),
        Err(original) => {
            let Ok(interpolation) = prepare_sampled_path(path, style) else {
                return Err(original);
            };
            let sampled = interpolation.interpolate(progress)
                .map_err(|error| GeometryError::Tessellation(error.to_string()))?;
            tessellate_path_mesh(&sampled, style, transform)
        }
    }
}

impl FramePreparer {
    pub(super) fn cache_path_mesh_at_progress(
        &mut self,
        path: &VectorPath,
        style: Style,
        transform: Transform2D,
        owner: SampledMeshOwner,
        progress: f32,
    ) -> Result<(usize, bool), GeometryError> {
        // Validate before touching either cache or packed presentation state.
        if !progress.is_finite() || !(0.0..=1.0).contains(&progress) {
            return Err(GeometryError::Tessellation("invalid path morph progress".into()));
        }
        let stroke_transform = path_stroke_transform_key(style, transform);
        let old_index = self.sampled_path_mesh_lookup.get(&owner).copied();
        if let Some(index) = old_index {
            let entry = &self.path_mesh_cache[index];
            if entry.path == *path
                && entry.stroke_transform == stroke_transform
                && entry.stroke_width_bits == style.stroke_width.to_bits()
                && entry.stroke_join == style.stroke_join
                && entry.stroke_cap == style.stroke_cap
                && entry.fill_enabled == style.fill.is_some()
            {
                let sampled = entry.sampled.as_ref().expect("sampled cache owner");
                if sampled.progress_bits == progress.to_bits() {
                    self.mark_path_mesh_used(index);
                    return Ok((index, false));
                }
                let current = sampled.interpolation.interpolate(progress)
                    .map_err(|error| GeometryError::Tessellation(error.to_string()))?;
                let mesh = tessellate_path_mesh(&current, style, transform)?;
                self.path_mesh_cache[index].mesh = mesh;
                self.path_mesh_cache[index].sampled.as_mut().unwrap().progress_bits =
                    progress.to_bits();
                self.path_mesh_cache_generation = self.path_mesh_cache_generation.saturating_add(1);
                self.mark_path_mesh_used(index);
                return Ok((index, true));
            }
        }

        // Do not retessellate or disable the retained GPU interpolation class.
        let original_error = match self.cache_path_mesh(path, style, transform) {
            Ok(cached) => return Ok(cached),
            Err(error) => error,
        };
        let Ok(interpolation) = prepare_sampled_path(path, style) else {
            return Err(original_error);
        };
        let current = interpolation.interpolate(progress)
            .map_err(|error| GeometryError::Tessellation(error.to_string()))?;
        let mesh = tessellate_path_mesh(&current, style, transform)?;
        let last_used = self.next_path_mesh_use();
        let entry = CachedPathMesh {
            path: path.clone(),
            stroke_transform,
            stroke_width_bits: style.stroke_width.to_bits(),
            stroke_join: style.stroke_join,
            stroke_cap: style.stroke_cap,
            fill_enabled: style.fill.is_some(),
            mesh,
            resident: None,
            last_used,
            sampled: Some(SampledPathMesh { owner, interpolation, progress_bits: progress.to_bits() }),
        };
        // Reuse the owner's one disposable mesh across progress AND later morphs.
        // Never insert it into the immutable, potentially shared mesh-key lookup.
        let index = if let Some(index) = old_index {
            self.path_mesh_cache[index] = entry;
            index
        } else {
            let index = self.path_mesh_cache.len();
            self.path_mesh_cache.push(entry);
            self.sampled_path_mesh_lookup.insert(owner, index);
            index
        };
        self.path_mesh_cache_generation = self.path_mesh_cache_generation.saturating_add(1);
        Ok((index, true))
    }
}
