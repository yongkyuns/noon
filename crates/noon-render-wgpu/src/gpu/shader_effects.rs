//! Device program residency and coordinate projection of the existing effective
//! effect column. No authored values, clocks or semantic identities live here.
use std::{collections::BTreeMap, fmt, sync::Arc};

use noon_core::{
    GeometryRef, GeometryResource, ObjectId, PublicationContext, Rect, ShaderCoordinates,
    VectorPath, ViewColorProfile,
};
use noon_runtime::RendererPublication;

use super::{EffectPrepareError, GpuRenderer, PreparedShaderProgram, ShaderSampleContext};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectProgramStats {
    pub rows_examined: usize,
    pub pipeline_compiles: usize,
}

#[derive(Debug)]
pub enum EffectProgramPrepareError {
    Profile(EffectPrepareError),
    Gpu(wgpu::Error),
}
impl fmt::Display for EffectProgramPrepareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Profile(e) => e.fmt(f),
            Self::Gpu(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for EffectProgramPrepareError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Profile(e) => Some(e),
            Self::Gpu(e) => Some(e),
        }
    }
}

#[derive(Debug)]
struct ResidentRow {
    object: ObjectId,
    programs: Vec<Arc<PreparedShaderProgram>>,
    path_bounds: Option<Box<PathBounds>>,
}

#[derive(Debug)]
struct PathBounds {
    path: Arc<VectorPath>,
    bounds: Option<Rect>,
}

#[derive(Debug, Default)]
pub(super) struct EffectPrograms {
    context: Option<PublicationContext>,
    device: Option<wgpu::Device>,
    rows: BTreeMap<usize, ResidentRow>,
}

impl GpuRenderer {
    /// Prepare new immutable effect programs before preparing/encoding a frame.
    /// Parameter-only publications retain the same programs. Residency follows
    /// attached execution rows even when those rows are temporarily offscreen.
    /// All row changes commit together after every requested program succeeds.
    pub async fn prepare_effect_programs(
        &mut self,
        device: &wgpu::Device,
        publication: &RendererPublication<'_>,
    ) -> Result<EffectProgramStats, EffectProgramPrepareError> {
        let same_device = self.effect_programs.device.as_ref() == Some(device);
        if same_device && self.effect_programs.context == Some(publication.context()) {
            return Ok(EffectProgramStats::default());
        }
        let full = !same_device
            || self.effect_programs.context.is_none()
            || publication.changes().is_all();
        let count = publication.frame().objects.len();
        let indices = (0..if full { count } else { 0 })
            .chain(
                full.then(|| self.effect_programs.rows.range(count..))
                    .into_iter()
                    .flatten()
                    .map(|(&i, _)| i),
            )
            .chain(
                (!full)
                    .then(|| publication.changes().object_indices())
                    .into_iter()
                    .flatten()
                    .copied(),
            );
        let mut edits = Vec::new();
        let mut stats = EffectProgramStats::default();
        let mut fresh = super::analytic_glow::EffectCaptureGpu::default();
        for index in indices {
            stats.rows_examined += 1;
            let previous = self.effect_programs.rows.get(&index);
            let row = publication
                .frame()
                .objects
                .get(index)
                .filter(|row| publication.object_index(row.id) == Some(index));
            let stack = row.and_then(|row| row.effects.as_deref());
            let shader_count = stack.map_or(0, |stack| {
                stack
                    .definitions()
                    .iter()
                    .filter(|d| d.as_shader().is_some())
                    .count()
            });
            if shader_count == 0 {
                if previous.is_some() {
                    edits.push((index, None));
                }
                continue;
            }
            let row = row.expect("checked shader row");
            if row.spatial.is_some()
                || publication.color_profile() != ViewColorProfile::Compatibility
                || self.target_format == wgpu::TextureFormat::Rgba16Float
            {
                return Err(EffectProgramPrepareError::Profile(
                    EffectPrepareError::UnsupportedCapture,
                ));
            }
            let sources = || {
                stack
                    .expect("checked shader stack")
                    .definitions()
                    .iter()
                    .filter_map(|d| d.as_shader())
                    .map(|s| s.program())
            };
            if previous.is_some_and(|old| {
                old.object == row.id
                    && old.programs.len() == shader_count
                    && old
                        .programs
                        .iter()
                        .zip(sources())
                        .all(|(old, source)| old.matches(device, source))
            }) {
                continue;
            }
            let filter = &mut self
                .effect_captures
                .as_deref_mut()
                .unwrap_or(&mut fresh)
                .filter;
            let mut programs = Vec::with_capacity(shader_count);
            for source in sources() {
                let (program, work) = filter
                    .prepare_shader_program(device, source)
                    .await
                    .map_err(EffectProgramPrepareError::Gpu)?;
                stats.pipeline_compiles += work.pipeline_compiles;
                programs.push(program);
            }
            edits.push((
                index,
                Some(ResidentRow {
                    object: row.id,
                    programs,
                    path_bounds: None,
                }),
            ));
        }
        if self.effect_captures.is_none()
            && !edits.is_empty()
            && edits.iter().any(|(_, row)| row.is_some())
        {
            self.effect_captures = Some(Box::new(fresh));
        }
        for (index, next) in edits {
            if let Some(row) = next {
                self.effect_programs.rows.insert(index, row);
            } else {
                self.effect_programs.rows.remove(&index);
            }
        }
        self.effect_programs.context = Some(publication.context());
        self.effect_programs.device = Some(device.clone());
        Ok(stats)
    }

    fn shader_path_bounds(&mut self, index: usize, path: &Arc<VectorPath>) -> Option<Rect> {
        let row = self.effect_programs.rows.get_mut(&index)?;
        if row
            .path_bounds
            .as_ref()
            .is_none_or(|cached| !Arc::ptr_eq(&cached.path, path))
        {
            row.path_bounds = Some(Box::new(PathBounds {
                path: Arc::clone(path),
                bounds: path.conservative_bounds(),
            }));
        }
        row.path_bounds.as_ref().and_then(|cached| cached.bounds)
    }

    /// Full-output-pixel maps from the same effective publication as the source.
    /// Capture translates these maps by its final padded integer tile origin.
    pub(super) fn effect_shader_context(
        &mut self,
        publication: &RendererPublication<'_>,
        index: usize,
    ) -> Result<ShaderSampleContext, EffectPrepareError> {
        let frame = publication.frame();
        let row = frame
            .objects
            .get(index)
            .ok_or(EffectPrepareError::PublicationMismatch)?;
        if self.viewport_size.contains(&0) || row.spatial.is_some() {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        let world = [
            f64::from(self.camera.world_size.x),
            f64::from(self.camera.world_size.y),
        ];
        let center = [
            f64::from(self.camera.center.x),
            f64::from(self.camera.center.y),
        ];
        let scene = [
            [
                world[0] / f64::from(self.viewport_size[0]),
                0.0,
                center[0] - world[0] * 0.5,
            ],
            [
                0.0,
                -world[1] / f64::from(self.viewport_size[1]),
                center[1] + world[1] * 0.5,
            ],
        ];
        // Only local programs need intrinsic bounds. Cache immutable path bounds
        // by Arc identity so parameter/pose edits never revisit path commands.
        // External resources resolve their current version to that same identity.
        let needs_local = row.effects.as_deref().is_some_and(|stack| {
            stack
                .definitions()
                .iter()
                .filter_map(|d| d.as_shader())
                .any(|s| {
                    s.program().schema().dependencies.coordinates
                        == ShaderCoordinates::LocalNormalized
                })
        });
        let geometry = frame.render_geometry(index);
        let path = match geometry {
            Some(GeometryRef::VectorPath(path)) => Some(path),
            Some(GeometryRef::External(id)) => {
                let resources = publication.geometry_resources();
                resources
                    .current_handle(*id)
                    .and_then(|h| resources.get(h))
                    .and_then(|r| match r {
                        GeometryResource::VectorPath(path) => Some(path),
                        GeometryResource::Mesh(_) => None,
                    })
            }
            _ => None,
        };
        if !needs_local || path.is_none() {
            if let Some(row) = self.effect_programs.rows.get_mut(&index) {
                row.path_bounds = None;
            }
        }
        let bounds = if needs_local {
            path.and_then(|path| self.shader_path_bounds(index, path))
                .or_else(|| {
                    geometry.and_then(|geometry| match geometry {
                        GeometryRef::VectorPath(_) | GeometryRef::External(_) => None,
                        geometry => geometry.local_bounds(),
                    })
                })
                .or(row.text_bounds)
                .or_else(|| row.content.image().map(|image| image.local_bounds()))
        } else {
            None
        };
        let pose = frame.render_transform(index);
        let local = bounds.and_then(|bounds| {
            let extent = bounds.max - bounds.min;
            let [sx, sy] = [f64::from(pose.scale.x), f64::from(pose.scale.y)];
            let [w, h] = [f64::from(extent.x), f64::from(extent.y)];
            if sx == 0.0 || sy == 0.0 || w <= 0.0 || h <= 0.0 {
                return None;
            }
            let (sin, cos) = f64::from(pose.rotation).sin_cos();
            let [tx, ty] = [f64::from(pose.translation.x), f64::from(pose.translation.y)];
            let inverse = [
                [
                    cos / sx / w,
                    sin / sx / w,
                    (-cos * tx - sin * ty) / sx / w - f64::from(bounds.min.x) / w,
                ],
                [
                    -sin / sy / h,
                    cos / sy / h,
                    (sin * tx - cos * ty) / sy / h - f64::from(bounds.min.y) / h,
                ],
            ];
            Some(inverse.map(|r| {
                [
                    r[0] * scene[0][0] + r[1] * scene[1][0],
                    r[0] * scene[0][1] + r[1] * scene[1][1],
                    r[0] * scene[0][2] + r[1] * scene[1][2] + r[2],
                ]
            }))
        });
        Ok(ShaderSampleContext {
            local_from_capture: local,
            scene_from_capture: scene,
            output_origin: [0.0; 2],
            authored_time: frame.time,
        })
    }
}
