//! Device program residency and coordinate projection of the existing effective
//! effect column. No authored values, clocks or semantic identities live here.
use std::{collections::BTreeMap, fmt, sync::Arc};

use noon_core::{
    GeometryRef, GeometryResource, ObjectId, PublicationContext, Rect, VectorPath, ViewColorProfile,
};
use noon_runtime::RendererPublication;

use super::{
    EffectPrepareError, GpuLease, GpuRenderer, PreparedShaderProgram, ShaderSampleContext,
};

mod preparation;
pub use preparation::{EffectProgramPreparation, PreparedEffectPrograms};

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
    programs: Vec<GpuLease<PreparedShaderProgram>>,
    path_bounds: Option<Box<PathBounds>>,
}

#[derive(Debug)]
struct PathBounds {
    path: Arc<VectorPath>,
    bounds: Option<Rect>,
}

#[derive(Debug, Default)]
pub(super) struct EffectPrograms {
    authority: Arc<()>,
    context: Option<PublicationContext>,
    device: Option<wgpu::Device>,
    rows: BTreeMap<usize, ResidentRow>,
}

impl GpuRenderer {
    /// A replacement working target can reuse immutable same-device programs.
    /// Only weak cache entries move; row bindings and cache authority stay fresh.
    pub fn copy_effect_program_cache_from(&mut self, device: &wgpu::Device, previous: &Self) {
        let Some(previous) = previous.effect_captures.as_deref() else {
            return;
        };
        if previous.filter.live_shader_programs() != 0 {
            self.effect_captures
                .get_or_insert_with(Default::default)
                .filter
                .copy_shader_program_cache_from(device, &previous.filter);
        }
    }

    /// O(1) host fast path before borrowing/cloning a sparse publication delta.
    pub fn effect_programs_current(
        &self,
        device: &wgpu::Device,
        context: PublicationContext,
    ) -> bool {
        self.effect_programs.device.as_ref() == Some(device)
            && self.effect_programs.context == Some(context)
    }

    /// Inspect only new/changed rows. The request owns immutable source/program
    /// leases, so an asynchronous host never borrows the live publication.
    pub fn effect_program_preparation(
        &self,
        device: &wgpu::Device,
        publication: &RendererPublication<'_>,
    ) -> Result<EffectProgramPreparation, EffectPrepareError> {
        let mut request = EffectProgramPreparation {
            authority: Arc::clone(&self.effect_programs.authority),
            device: device.clone(),
            context: Some(publication.context()),
            rows: Vec::new(),
            modules: Vec::new(),
            stats: EffectProgramStats::default(),
            missing: false,
        };
        let same_device = self.effect_programs.device.as_ref() == Some(device);
        if same_device && self.effect_programs.context == Some(publication.context()) {
            return Ok(request);
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
        for index in indices {
            request.stats.rows_examined += 1;
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
                    request.rows.push((index, None));
                }
                continue;
            }
            let row = row.expect("checked shader row");
            if row.spatial.is_some()
                || publication.color_profile() != ViewColorProfile::Compatibility
                || self.target_format == wgpu::TextureFormat::Rgba16Float
            {
                return Err(EffectPrepareError::UnsupportedCapture);
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
            let programs = sources()
                .map(|source| {
                    let program = self
                        .effect_captures
                        .as_deref()
                        .and_then(|cache| cache.filter.shader_program(device, source).ok());
                    request.missing |= program.is_none();
                    (source.clone(), program)
                })
                .collect();
            request.rows.push((
                index,
                Some(preparation::RowRequest {
                    object: row.id,
                    programs,
                }),
            ));
        }
        Ok(request)
    }

    /// Prepare immutable requested modules before any semantic/runtime revision
    /// exists. No speculative renderer row or object identity is manufactured.
    pub fn effect_module_preparation(
        &self,
        device: &wgpu::Device,
        programs: &[noon_core::WgslEffectProgram],
    ) -> EffectProgramPreparation {
        let modules: Vec<_> = programs
            .iter()
            .map(|source| {
                let prepared = self
                    .effect_captures
                    .as_deref()
                    .and_then(|cache| cache.filter.shader_program(device, source).ok());
                (source.clone(), prepared)
            })
            .collect();
        EffectProgramPreparation {
            authority: Arc::clone(&self.effect_programs.authority),
            device: device.clone(),
            context: None,
            rows: Vec::new(),
            missing: modules.iter().any(|(_, p)| p.is_none()),
            modules,
            stats: EffectProgramStats::default(),
        }
    }

    /// Make validated immutable modules available to a newer publication without
    /// installing stale row bindings. The caller keeps this lease through replanning.
    pub fn cache_effect_programs(
        &mut self,
        device: &wgpu::Device,
        prepared: &PreparedEffectPrograms,
    ) -> Result<(), EffectPrepareError> {
        if prepared.device != *device
            || !Arc::ptr_eq(&prepared.authority, &self.effect_programs.authority)
        {
            return Err(EffectPrepareError::PublicationMismatch);
        }
        for program in &prepared.modules {
            self.effect_captures
                .get_or_insert_with(Default::default)
                .filter
                .remember_shader_program(program);
        }
        for (_, row) in &prepared.rows {
            if let Some(row) = row {
                let cache = self.effect_captures.get_or_insert_with(Default::default);
                for program in &row.programs {
                    cache.filter.remember_shader_program(program);
                }
            }
        }
        Ok(())
    }

    /// Install row residency only for the exact current publication/device/cache.
    /// Failed or stale preparation never consumes runtime changes or old output.
    pub fn commit_effect_programs(
        &mut self,
        device: &wgpu::Device,
        publication: &RendererPublication<'_>,
        prepared: PreparedEffectPrograms,
    ) -> Result<EffectProgramStats, EffectPrepareError> {
        if prepared.context != Some(publication.context()) {
            return Err(EffectPrepareError::PublicationMismatch);
        }
        self.cache_effect_programs(device, &prepared)?;
        for (index, row) in prepared.rows {
            if let Some(mut row) = row {
                if let Some(old) = self
                    .effect_programs
                    .rows
                    .get_mut(&index)
                    .filter(|old| old.object == row.object)
                {
                    row.path_bounds = old.path_bounds.take();
                }
                self.effect_programs.rows.insert(index, row);
            } else {
                self.effect_programs.rows.remove(&index);
            }
        }
        self.effect_programs.context = prepared.context;
        self.effect_programs.device = Some(device.clone());
        Ok(prepared.stats)
    }

    /// Native/initialization convenience over the same owned preparation seam.
    pub async fn prepare_effect_programs(
        &mut self,
        device: &wgpu::Device,
        publication: &RendererPublication<'_>,
    ) -> Result<EffectProgramStats, EffectProgramPrepareError> {
        let prepared = self
            .effect_program_preparation(device, publication)
            .map_err(EffectProgramPrepareError::Profile)?
            .prepare()
            .await?;
        self.commit_effect_programs(device, publication, prepared)
            .map_err(EffectProgramPrepareError::Profile)
    }

    /// Diagnostic observation: attached shader rows and live cached device programs.
    /// The latter scans only the program cache; ordinary preparation never uses it.
    pub fn resident_effect_program_counts(&self) -> [usize; 2] {
        [
            self.effect_programs.rows.len(),
            self.effect_captures
                .as_deref()
                .map_or(0, |cache| cache.filter.live_shader_programs()),
        ]
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
        let needs_local = row
            .effects
            .as_deref()
            .is_some_and(noon_compile::CompiledEffectStack::uses_local_shader_coordinates);
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
