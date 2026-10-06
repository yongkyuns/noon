//! Disposable mesh residency derived from the existing coherent runtime publication.
//! Local changes inspect only dirty execution rows. wgpu retains old buffers for
//! already encoded/submitted work when resource references are dropped here.

mod boundary;
mod cairo;

use super::spatial_path::{SpatialPathError, SpatialPathGpuState};
use super::{create_buffer_with_data, DrawStats};
use bytemuck::{Pod, Zeroable};
use noon_core::{
    GeometryRef, GeometryResource, GeometryResourceHandle, GeometryResourceLookup, MeshResource,
    SemanticCamera3D, SemanticSpatialMaterial, SemanticVec3, SemanticWorldTransform3D,
};
use noon_runtime::{FrameChanges, FrameState, RendererPublication};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    mem::{size_of, size_of_val},
    sync::{Arc, OnceLock},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpatialUploadStats {
    pub rows_visited: usize,
    pub geometry_bytes: usize,
    pub instance_bytes: usize,
    pub camera_bytes: usize,
    pub light_bytes: usize,
    pub resident_meshes: usize,
    pub resident_instances: usize,
}
impl SpatialUploadStats {
    pub const fn bytes_uploaded(self) -> usize {
        self.geometry_bytes + self.instance_bytes + self.camera_bytes + self.light_bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpatialPrepareError {
    MissingCamera,
    MultipleCameras,
    MissingMesh(usize),
    InvalidWorld(usize),
    InvalidCamera,
    UnrepresentableVertex,
    TransparentMesh(usize),
    MeshStroke(usize),
    MissingPointLight,
    MultiplePointLights,
    PointLitMeshNeedsNormals(usize),
    MissingCairoAppearance(usize),
    SpatialPath(SpatialPathError),
    BufferLimit,
    StalePublication,
}
impl std::fmt::Display for SpatialPrepareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid spatial renderer publication: {self:?}")
    }
}
impl std::error::Error for SpatialPrepareError {}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct Instance {
    world: [f32; 16],
    normals: [[f32; 4]; 3],
    color: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
struct LightUniform {
    position_enabled: [f32; 4],
    color_intensity: [f32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct PointLight {
    position: [f32; 3],
    color_intensity: [f32; 4],
}

#[derive(Debug)]
struct ResidentMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    users: usize,
    boundary: Option<(wgpu::Buffer, u32)>,
    cairo: Option<wgpu::BindGroup>,
}
#[derive(Debug)]
struct Draw {
    handle: GeometryResourceHandle,
    instance: usize,
    material: SemanticSpatialMaterial,
    transparent: bool,
    center: SemanticVec3,
    stroke: Option<Instance>,
}
struct StagedDraw {
    handle: GeometryResourceHandle,
    mesh: Arc<MeshResource>,
    instance: Instance,
    material: SemanticSpatialMaterial,
    transparent: bool,
    center: SemanticVec3,
    stroke: Option<Instance>,
}
#[derive(Debug)]
struct GpuState {
    device: wgpu::Device,
    camera_layout: wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    cairo: Option<cairo::Pipelines>,
    viewport: [u32; 2],
    pipeline: wgpu::RenderPipeline,
    pipeline_msaa: wgpu::RenderPipeline,
    transparent_pipeline: wgpu::RenderPipeline,
    transparent_pipeline_msaa: wgpu::RenderPipeline,
    boundary_pipeline: wgpu::RenderPipeline,
    boundary_pipeline_msaa: wgpu::RenderPipeline,
    boundary_metrics: wgpu::Buffer,
    boundary_metrics_value: Option<[f32; 4]>,
    stroke_instances: Option<wgpu::Buffer>,
    stroke_capacity: usize,
    camera: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    light: wgpu::Buffer,
    light_value: Option<PointLight>,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    depth: wgpu::TextureView,
    depth_msaa: OnceLock<wgpu::TextureView>,
}

#[derive(Debug, Default)]
pub(super) struct SpatialGpuState {
    last_publication: Option<noon_core::PublicationContext>,
    initialized: bool,
    gpu: Option<GpuState>,
    draws: BTreeMap<usize, Draw>,
    cameras: BTreeMap<usize, SemanticCamera3D>,
    light_object: Option<usize>,
    light: Option<PointLight>,
    point_lit_draws: usize,
    meshes: HashMap<GeometryResourceHandle, ResidentMesh>,
    mesh_instances: BTreeMap<(GeometryResourceHandle, bool), InstanceRanges>,
    transparent_draws: BTreeSet<usize>,
    transparent_order: Vec<usize>,
    stroked_draws: BTreeSet<usize>,
    instances: Vec<Instance>,
    free_instances: Vec<usize>,
    camera_matrix: Option<[f32; 16]>,
    viewport: [u32; 2],
    planar_camera: Option<super::Camera2D>,
    paths: SpatialPathGpuState,
}

impl SpatialGpuState {
    pub fn reset_publication_context(&mut self) {
        self.last_publication = None;
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        viewport: [u32; 2],
        publication: &RendererPublication<'_>,
        camera: super::Camera2D,
    ) -> Result<SpatialUploadStats, SpatialPrepareError> {
        self.prepare_with_resources(
            device,
            queue,
            format,
            viewport,
            publication.context(),
            publication.frame(),
            publication.changes(),
            publication.geometry_resources(),
            publication.text_resources(),
            publication.font_resources(),
            publication.painter_order(),
            camera,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_with_resources(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        viewport: [u32; 2],
        context: noon_core::PublicationContext,
        frame: &FrameState,
        changes: &FrameChanges,
        geometry_resources: &dyn GeometryResourceLookup,
        text_resources: &dyn noon_core::TextResourceLookup,
        font_resources: &dyn noon_core::FontResourceLookup,
        painter_order: &[u32],
        camera: super::Camera2D,
    ) -> Result<SpatialUploadStats, SpatialPrepareError> {
        if self
            .last_publication
            .is_some_and(|applied| super::retained_text::publication_is_stale(context, applied))
        {
            return Err(SpatialPrepareError::StalePublication);
        }
        let retry = self.last_publication == Some(context);
        if retry && self.viewport == viewport && self.planar_camera == Some(camera) {
            return Ok(SpatialUploadStats {
                resident_meshes: self.meshes.len(),
                resident_instances: self.draws.len() + self.paths.draw_count(),
                ..Default::default()
            });
        }
        let clean = FrameChanges::default();
        let result = self.prepare_frame(
            device,
            queue,
            format,
            viewport,
            frame,
            if retry { &clean } else { changes },
            geometry_resources,
            text_resources,
            font_resources,
            painter_order,
            camera,
        )?;
        self.last_publication = Some(context);
        self.planar_camera = Some(camera);
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        viewport: [u32; 2],
        frame: &FrameState,
        changes: &FrameChanges,
        resources: &dyn GeometryResourceLookup,
        text_resources: &dyn noon_core::TextResourceLookup,
        font_resources: &dyn noon_core::FontResourceLookup,
        painter_order: &[u32],
        camera: super::Camera2D,
    ) -> Result<SpatialUploadStats, SpatialPrepareError> {
        let full = !self.initialized || changes.is_all();
        let mut indices: BTreeSet<_> = if full {
            (0..frame.objects.len())
                .chain(self.draws.keys().copied())
                .chain(self.paths.draw_indices())
                .chain(self.cameras.keys().copied())
                .collect()
        } else {
            changes
                .object_indices()
                .iter()
                .chain(changes.added_indices())
                .chain(changes.removed_indices())
                .copied()
                .collect()
        };
        if changes.has_painter_order_change() {
            indices.extend(self.paths.draw_indices());
            indices.extend(
                frame
                    .objects
                    .iter()
                    .enumerate()
                    .filter_map(|(index, object)| {
                        frame
                            .is_present(index)
                            .then_some(object.spatial.as_deref())
                            .flatten()
                            .filter(|spatial| {
                                spatial.composition_domain
                                    == noon_core::SemanticSpatialCompositionDomain::FixedOrientation
                            })
                            .map(|_| index)
                    }),
            );
        }
        // Stage and validate the planar lane before mesh residency, buffer
        // allocation, or queue writes. Its commit is infallible after this point.
        let path_plan = self
            .paths
            .plan(
                device,
                frame,
                &indices,
                changes,
                camera,
                painter_order,
                resources,
                text_resources,
                font_resources,
            )
            .map_err(SpatialPrepareError::SpatialPath)?;
        let mut cameras = self.cameras.clone(); // at most one admitted camera
        let touched_existing_light = self
            .light_object
            .is_some_and(|index| indices.contains(&index));
        let mut light_object = if touched_existing_light {
            None
        } else {
            self.light_object
        };
        let mut point_light = if touched_existing_light {
            None
        } else {
            self.light
        };
        let mut staged = Vec::new();
        for &index in &indices {
            cameras.remove(&index);
            if light_object == Some(index) {
                light_object = None;
                point_light = None;
            }
            let mut draw = None;
            if let Some(object) = frame.objects.get(index).filter(|_| frame.is_present(index)) {
                if let Some(spatial) = object.spatial.as_deref() {
                    let fixed_orientation = spatial.composition_domain
                        == noon_core::SemanticSpatialCompositionDomain::FixedOrientation;
                    if spatial.fixed_orientation_center.is_some() != fixed_orientation {
                        return Err(SpatialPrepareError::InvalidWorld(index));
                    }
                    if (spatial.point_light || spatial.camera_projection.is_some())
                        && spatial.composition_domain
                            != noon_core::SemanticSpatialCompositionDomain::World
                    {
                        return Err(SpatialPrepareError::InvalidWorld(index));
                    }
                    if fixed_orientation
                        && spatial.draw_kind != noon_compile::CompiledSpatialDrawKind::Planar
                    {
                        return Err(SpatialPrepareError::SpatialPath(
                            SpatialPathError::UnsupportedGeometry,
                        ));
                    }
                    if spatial.draw_kind == noon_compile::CompiledSpatialDrawKind::Mesh
                        && spatial.composition_domain
                            != noon_core::SemanticSpatialCompositionDomain::World
                    {
                        return Err(SpatialPrepareError::InvalidWorld(index));
                    }
                    if spatial.point_light {
                        if light_object.is_some() {
                            return Err(SpatialPrepareError::MultiplePointLights);
                        }
                        let color = object
                            .style
                            .fill
                            .ok_or(SpatialPrepareError::InvalidWorld(index))?;
                        let intensity = color.alpha * object.style.opacity * object.appearance;
                        let light = PointLight {
                            position: [
                                spatial.world.translation.x as f32,
                                spatial.world.translation.y as f32,
                                spatial.world.translation.z as f32,
                            ],
                            color_intensity: [color.red, color.green, color.blue, intensity],
                        };
                        if light
                            .position
                            .iter()
                            .chain(light.color_intensity.iter())
                            .any(|value| !value.is_finite())
                            || !(0.0..=1.0).contains(&intensity)
                        {
                            return Err(SpatialPrepareError::InvalidWorld(index));
                        }
                        light_object = Some(index);
                        point_light = Some(light);
                    } else if let Some(projection) = spatial.camera_projection {
                        let camera = SemanticCamera3D::new(
                            spatial.world.translation,
                            spatial.world.rotation,
                            projection,
                        )
                        .ok_or(SpatialPrepareError::InvalidCamera)?;
                        cameras.insert(index, camera);
                    } else if spatial.draw_kind == noon_compile::CompiledSpatialDrawKind::Mesh {
                        let Some(GeometryRef::External(id)) = object.content.geometry() else {
                            return Err(SpatialPrepareError::MissingMesh(index));
                        };
                        let handle = resources
                            .current_handle(*id)
                            .ok_or(SpatialPrepareError::MissingMesh(index))?;
                        let Some(GeometryResource::Mesh(mesh)) = resources.get(handle) else {
                            return Err(SpatialPrepareError::MissingMesh(index));
                        };
                        if spatial.material == SemanticSpatialMaterial::PointLit {
                            validate_point_lit_normals(mesh, index)?;
                        }
                        if spatial.material == SemanticSpatialMaterial::CairoSurface
                            && mesh.cairo_appearance().is_none()
                        {
                            return Err(SpatialPrepareError::MissingCairoAppearance(index));
                        }
                        let color = object.style.fill.unwrap_or(noon_core::Color::TRANSPARENT);
                        let alpha = color.alpha * object.style.opacity * object.appearance;
                        if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
                            return Err(SpatialPrepareError::TransparentMesh(index));
                        }
                        // Object-depth ordering is well-defined for a retained
                        // face family. Arbitrary multi-face translucent meshes
                        // require a different ordering contract and remain rejected.
                        if alpha > 0.0 && alpha < 1.0 && !mesh.is_single_face() {
                            return Err(SpatialPrepareError::TransparentMesh(index));
                        }
                        let stroke = object
                            .style
                            .stroke
                            .filter(|_| object.style.stroke_width > 0.0)
                            .map(|color| {
                                let alpha = color.alpha * object.style.opacity * object.appearance;
                                let mut result = instance(
                                    spatial.world,
                                    [color.red, color.green, color.blue, alpha],
                                    if spatial.material == SemanticSpatialMaterial::CairoSurface {
                                        spatial.material
                                    } else {
                                        SemanticSpatialMaterial::Unlit
                                    },
                                )
                                .ok_or(SpatialPrepareError::InvalidWorld(index))?;
                                let scale = match object.style.stroke_width_mode {
                                    noon_core::StrokeWidthMode::ScreenSpace => 1.0,
                                    noon_core::StrokeWidthMode::ScaleWithObject => spatial
                                        .world
                                        .scale
                                        .x
                                        .abs()
                                        .max(spatial.world.scale.y.abs())
                                        .max(spatial.world.scale.z.abs())
                                        as f32,
                                };
                                result.normals[1][3] = object.style.stroke_width * scale;
                                if !result.normals[1][3].is_finite()
                                    || !alpha.is_finite()
                                    || !(0.0..=1.0).contains(&alpha)
                                {
                                    return Err(SpatialPrepareError::MeshStroke(index));
                                }
                                Ok(result)
                            })
                            .transpose()?;
                        if alpha == 0.0 && stroke.is_none() {
                            staged.push((index, None));
                            continue;
                        }
                        let bounds = mesh.bounds();
                        let local_center = SemanticVec3::new(
                            bounds.min.x * 0.5 + bounds.max.x * 0.5,
                            bounds.min.y * 0.5 + bounds.max.y * 0.5,
                            bounds.min.z * 0.5 + bounds.max.z * 0.5,
                        );
                        let center = spatial
                            .world
                            .transform_point(local_center)
                            .ok_or(SpatialPrepareError::InvalidWorld(index))?;
                        let instance = instance(
                            spatial.world,
                            [color.red, color.green, color.blue, alpha],
                            spatial.material,
                        )
                        .ok_or(SpatialPrepareError::InvalidWorld(index))?;
                        draw = Some(StagedDraw {
                            handle,
                            mesh: mesh.clone(),
                            instance,
                            material: spatial.material,
                            transparent: alpha < 1.0,
                            center,
                            stroke,
                        });
                    }
                }
            }
            staged.push((index, draw));
        }
        if cameras.len() > 1 {
            return Err(SpatialPrepareError::MultipleCameras);
        }
        let remaining_draws = self.draws.len() as isize
            + staged
                .iter()
                .map(|(index, draw)| {
                    isize::from(draw.is_some()) - isize::from(self.draws.contains_key(index))
                })
                .sum::<isize>();
        let remaining_paths = self.paths.remaining_after(&path_plan);
        let mut remaining_point_lit_draws = self.point_lit_draws;
        for (index, draw) in &staged {
            if self
                .draws
                .get(index)
                .is_some_and(|draw| draw.material == SemanticSpatialMaterial::PointLit)
            {
                remaining_point_lit_draws -= 1;
            }
            if draw
                .as_ref()
                .is_some_and(|draw| draw.material == SemanticSpatialMaterial::PointLit)
            {
                remaining_point_lit_draws += 1;
            }
        }
        if remaining_point_lit_draws > 0 && point_light.is_none() {
            return Err(SpatialPrepareError::MissingPointLight);
        }
        let matrix = match cameras.values().next().copied() {
            Some(camera) => Some(
                lower_matrix(
                    camera
                        .camera_matrix(f64::from(viewport[0]) / f64::from(viewport[1]))
                        .ok_or(SpatialPrepareError::InvalidCamera)?,
                )
                .ok_or(SpatialPrepareError::InvalidCamera)?,
            ),
            None if remaining_draws > 0 || remaining_paths > 0 => {
                return Err(SpatialPrepareError::MissingCamera)
            }
            None => None,
        };
        // Validate every new resource and allocation before changing residency or issuing writes.
        let mut new_meshes = HashMap::new();
        for (_, draw) in &staged {
            if let Some(draw) = draw {
                if !self.meshes.contains_key(&draw.handle) && !new_meshes.contains_key(&draw.handle)
                {
                    let vertices = lower_vertices(&draw.mesh)?;
                    let limit = device.limits().max_buffer_size as usize;
                    if size_of_val(vertices.as_slice()) > limit
                        || size_of_val(draw.mesh.indices()) > limit
                        || u32::try_from(draw.mesh.indices().len()).is_err()
                    {
                        return Err(SpatialPrepareError::BufferLimit);
                    }
                    new_meshes.insert(draw.handle, (vertices, draw.mesh.clone()));
                }
            }
        }
        let mut new_cairo = HashMap::new();
        for (index, draw) in &staged {
            if let Some(draw) = draw
                .as_ref()
                .filter(|draw| draw.material == SemanticSpatialMaterial::CairoSurface)
            {
                if !self
                    .meshes
                    .get(&draw.handle)
                    .is_some_and(|mesh| mesh.cairo.is_some())
                    && !new_cairo.contains_key(&draw.handle)
                {
                    let appearance = draw
                        .mesh
                        .cairo_appearance()
                        .ok_or(SpatialPrepareError::MissingCairoAppearance(*index))?;
                    let value = cairo::Uniform::lower(appearance)?;
                    if size_of::<cairo::Uniform>() > device.limits().max_buffer_size as usize {
                        return Err(SpatialPrepareError::BufferLimit);
                    }
                    new_cairo.insert(draw.handle, value);
                }
            }
        }
        let mut new_boundaries = HashMap::new();
        for (_, draw) in &staged {
            if let Some(draw) = draw.as_ref().filter(|draw| draw.stroke.is_some()) {
                if !self
                    .meshes
                    .get(&draw.handle)
                    .is_some_and(|mesh| mesh.boundary.is_some())
                    && !new_boundaries.contains_key(&draw.handle)
                {
                    let edges = boundary::vertices(&draw.mesh)?;
                    if size_of_val(edges.as_slice()) > device.limits().max_buffer_size as usize
                        || u32::try_from(edges.len()).is_err()
                    {
                        return Err(SpatialPrepareError::BufferLimit);
                    }
                    new_boundaries.insert(draw.handle, edges);
                }
            }
        }
        let additions = staged
            .iter()
            .filter(|(index, draw)| draw.is_some() && !self.draws.contains_key(index))
            .count();
        let needed = self
            .instances
            .len()
            .saturating_add(additions.saturating_sub(self.free_instances.len()));
        let capacity = needed
            .max(1)
            .checked_next_power_of_two()
            .ok_or(SpatialPrepareError::BufferLimit)?;
        if u32::try_from(needed).is_err()
            || capacity
                .checked_mul(size_of::<Instance>())
                .ok_or(SpatialPrepareError::BufferLimit)?
                > device.limits().max_buffer_size as usize
        {
            return Err(SpatialPrepareError::BufferLimit);
        }

        let mut stats = SpatialUploadStats {
            rows_visited: indices.len(),
            ..Default::default()
        };
        if self.gpu.is_none() && (remaining_draws > 0 || remaining_paths > 0) {
            self.gpu = Some(GpuState::new(device, queue, format, viewport));
            self.viewport = viewport;
        }
        let path_stats = if remaining_paths > 0 || self.paths.is_active() {
            let gpu = self
                .gpu
                .as_ref()
                .expect("active spatial paths need GPU state");
            self.paths.commit(
                device,
                queue,
                &gpu.camera_layout,
                &gpu.camera_group,
                format,
                path_plan,
            )
        } else {
            Default::default()
        };
        stats.geometry_bytes += path_stats.geometry_bytes;
        stats.instance_bytes += path_stats.instance_bytes;
        stats.camera_bytes += path_stats.camera_bytes;
        for (handle, (vertices, mesh)) in new_meshes {
            let vertex_bytes = bytemuck::cast_slice(&vertices);
            let index_bytes = bytemuck::cast_slice(mesh.indices());
            self.meshes.insert(
                handle,
                ResidentMesh {
                    vertices: create_buffer_with_data(
                        device,
                        queue,
                        Some("Noon immutable mesh vertices"),
                        vertex_bytes,
                        wgpu::BufferUsages::VERTEX,
                    ),
                    indices: create_buffer_with_data(
                        device,
                        queue,
                        Some("Noon immutable mesh indices"),
                        index_bytes,
                        wgpu::BufferUsages::INDEX,
                    ),
                    index_count: mesh.indices().len() as u32,
                    users: 0,
                    boundary: None,
                    cairo: None,
                },
            );
            stats.geometry_bytes += vertex_bytes.len() + index_bytes.len();
        }
        if !new_cairo.is_empty() {
            let gpu = self.gpu.as_mut().expect("Cairo draws require GPU state");
            let pipelines = gpu.cairo.get_or_insert_with(|| {
                cairo::Pipelines::new(device, &gpu.camera_layout, gpu.format)
            });
            for (handle, appearance) in new_cairo {
                self.meshes
                    .get_mut(&handle)
                    .expect("staged Cairo resource")
                    .cairo = Some(pipelines.retain(device, queue, appearance));
                stats.geometry_bytes += size_of::<cairo::Uniform>();
            }
        }
        for (handle, edges) in new_boundaries {
            let bytes = bytemuck::cast_slice(&edges);
            let buffer = if bytes.is_empty() {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Noon empty mesh boundary"),
                    size: 4,
                    usage: wgpu::BufferUsages::VERTEX,
                    mapped_at_creation: false,
                })
            } else {
                create_buffer_with_data(
                    device,
                    queue,
                    Some("Noon immutable mesh boundary"),
                    bytes,
                    wgpu::BufferUsages::VERTEX,
                )
            };
            self.meshes
                .get_mut(&handle)
                .expect("staged boundary resource")
                .boundary = Some((buffer, edges.len() as u32));
            stats.geometry_bytes += bytes.len();
        }
        let mut dirty_strokes = Vec::new();
        let mut transparent_changed =
            matrix != self.camera_matrix || changes.has_painter_order_change();
        let mut dirty = BTreeSet::new();
        let mut released = BTreeSet::new();
        for (index, staged) in staged {
            let previous = self.draws.remove(&index);
            let same_membership = previous
                .as_ref()
                .zip(staged.as_ref())
                .is_some_and(|(old, new)| old.handle == new.handle);
            transparent_changed |= previous.as_ref().is_some_and(|draw| draw.transparent)
                || staged.as_ref().is_some_and(|draw| draw.transparent);
            self.transparent_draws.remove(&index);
            self.stroked_draws.remove(&index);
            let same_opaque_membership =
                previous
                    .as_ref()
                    .zip(staged.as_ref())
                    .is_some_and(|(old, new)| {
                        old.handle == new.handle
                            && !old.transparent
                            && !new.transparent
                            && is_cairo(old.material) == is_cairo(new.material)
                    });
            if let Some(old) = previous
                .as_ref()
                .filter(|old| !old.transparent && !same_opaque_membership)
            {
                self.mesh_instances
                    .get_mut(&(old.handle, is_cairo(old.material)))
                    .expect("resident opaque group")
                    .remove(old.instance);
            }
            if let Some(old) = previous.as_ref().filter(|_| !same_membership) {
                self.meshes
                    .get_mut(&old.handle)
                    .expect("resident draw")
                    .users -= 1;
                released.insert(old.handle);
            }
            if let Some(staged) = staged {
                let slot = previous.map(|old| old.instance).unwrap_or_else(|| {
                    self.free_instances.pop().unwrap_or_else(|| {
                        self.instances.push(Instance::zeroed());
                        self.instances.len() - 1
                    })
                });
                if self.instances[slot] != staged.instance {
                    self.instances[slot] = staged.instance;
                    dirty.insert(slot);
                }
                if !same_membership {
                    self.meshes
                        .get_mut(&staged.handle)
                        .expect("staged mesh")
                        .users += 1;
                }
                if !staged.transparent && !same_opaque_membership {
                    self.mesh_instances
                        .entry((staged.handle, is_cairo(staged.material)))
                        .or_default()
                        .insert(slot);
                }
                if staged.transparent {
                    self.transparent_draws.insert(index);
                }
                if let Some(stroke) = staged.stroke {
                    self.stroked_draws.insert(index);
                    dirty_strokes.push((slot, stroke));
                }
                self.draws.insert(
                    index,
                    Draw {
                        handle: staged.handle,
                        instance: slot,
                        material: staged.material,
                        transparent: staged.transparent,
                        center: staged.center,
                        stroke: staged.stroke,
                    },
                );
            } else if let Some(old) = previous {
                self.free_instances.push(old.instance);
            }
        }
        for handle in released {
            if self.meshes.get(&handle).is_some_and(|mesh| mesh.users == 0) {
                self.meshes.remove(&handle);
                self.mesh_instances.remove(&(handle, false));
                self.mesh_instances.remove(&(handle, true));
            }
        }
        if transparent_changed {
            self.transparent_order = self.transparent_draws.iter().copied().collect();
            if let Some(camera) = cameras.values().next() {
                let depth = |index: usize| {
                    let p = self.draws[&index].center;
                    camera
                        .orientation
                        .inverse()
                        .rotate_vector(SemanticVec3::new(
                            p.x - camera.position.x,
                            p.y - camera.position.y,
                            p.z - camera.position.z,
                        ))
                        .map_or(f64::INFINITY, |p| p.z)
                };
                // Camera looks along -Z: more negative view Z is drawn first.
                self.transparent_order.sort_by(|a, b| {
                    depth(*a).total_cmp(&depth(*b)).then_with(|| {
                        let rank = |index: &usize| {
                            self.paths.painter_rank(*index).unwrap_or(*index as u32)
                        };
                        rank(a).cmp(&rank(b)).then(a.cmp(b))
                    })
                });
            }
        }
        self.cameras = cameras;
        if let Some(gpu) = &mut self.gpu {
            if self.viewport != viewport {
                gpu.resize(device, viewport);
            }
            if matrix != self.camera_matrix {
                if let Some(matrix) = matrix {
                    queue.write_buffer(&gpu.camera, 0, bytemuck::cast_slice(&matrix));
                    stats.camera_bytes += size_of_val(&matrix);
                }
            }
            if point_light != gpu.light_value {
                let uniform =
                    point_light.map_or_else(LightUniform::default, |light| LightUniform {
                        position_enabled: [
                            light.position[0],
                            light.position[1],
                            light.position[2],
                            1.0,
                        ],
                        color_intensity: light.color_intensity,
                    });
                queue.write_buffer(&gpu.light, 0, bytemuck::bytes_of(&uniform));
                stats.light_bytes = size_of::<LightUniform>();
                gpu.light_value = point_light;
            }
            let metrics = [
                viewport[0] as f32,
                viewport[1] as f32,
                2.0 / camera.world_size.x,
                2.0 / camera.world_size.y,
            ];
            if (!self.stroked_draws.is_empty()
                || gpu.cairo.is_some()
                || self.paths.has_cairo_draws())
                && gpu.boundary_metrics_value != Some(metrics)
            {
                queue.write_buffer(&gpu.boundary_metrics, 0, bytemuck::cast_slice(&metrics));
                gpu.boundary_metrics_value = Some(metrics);
                stats.camera_bytes += size_of_val(&metrics);
            }
            if !self.stroked_draws.is_empty()
                && (gpu.stroke_instances.is_none() || needed > gpu.stroke_capacity)
            {
                gpu.stroke_instances = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Noon mesh boundary instances"),
                    size: (capacity * size_of::<Instance>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
                gpu.stroke_capacity = capacity;
                dirty_strokes = self
                    .stroked_draws
                    .iter()
                    .map(|index| {
                        let draw = &self.draws[index];
                        (draw.instance, draw.stroke.expect("stroked draw"))
                    })
                    .collect();
            }
            if let Some(buffer) = gpu.stroke_instances.as_ref() {
                for (slot, stroke) in dirty_strokes {
                    queue.write_buffer(
                        buffer,
                        (slot * size_of::<Instance>()) as u64,
                        bytemuck::bytes_of(&stroke),
                    );
                    stats.instance_bytes += size_of::<Instance>();
                }
            }
            if self.stroked_draws.is_empty() {
                gpu.stroke_instances = None;
                gpu.stroke_capacity = 0;
            }
            if needed > gpu.instance_capacity {
                gpu.instances = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Noon mesh instances"),
                    size: (capacity * size_of::<Instance>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                gpu.instance_capacity = capacity;
                if !self.instances.is_empty() {
                    let bytes = bytemuck::cast_slice(&self.instances);
                    queue.write_buffer(&gpu.instances, 0, bytes);
                    stats.instance_bytes += bytes.len();
                }
            } else {
                let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
                for slot in dirty {
                    if let Some(last) = ranges.last_mut().filter(|range| range.end == slot) {
                        last.end += 1;
                    } else {
                        ranges.push(slot..slot + 1);
                    }
                }
                for range in ranges {
                    let bytes = bytemuck::cast_slice(&self.instances[range.clone()]);
                    queue.write_buffer(
                        &gpu.instances,
                        (range.start * size_of::<Instance>()) as u64,
                        bytes,
                    );
                    stats.instance_bytes += bytes.len();
                }
            }
        }
        self.light_object = light_object;
        self.light = point_light;
        self.point_lit_draws = remaining_point_lit_draws;
        self.viewport = viewport;
        self.camera_matrix = matrix;
        self.initialized = true;
        stats.resident_meshes = self.meshes.len();
        stats.resident_instances = self.draws.len() + self.paths.draw_count();
        Ok(stats)
    }

    fn encode_boundary<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        gpu: &'a GpuState,
        draw: &Draw,
        sample_count: u32,
    ) -> usize {
        if draw.stroke.is_none() {
            return 0;
        }
        let Some((vertices, count)) = &self.meshes[&draw.handle].boundary else {
            return 0;
        };
        if *count == 0 {
            return 0;
        }
        if is_cairo(draw.material) {
            pass.set_pipeline(gpu.cairo.as_ref().expect("Cairo pipelines").select(
                sample_count,
                true,
                true,
            ));
            pass.set_bind_group(
                1,
                self.meshes[&draw.handle]
                    .cairo
                    .as_ref()
                    .expect("Cairo appearance"),
                &[],
            );
        } else {
            pass.set_pipeline(if sample_count == 1 {
                &gpu.boundary_pipeline
            } else {
                &gpu.boundary_pipeline_msaa
            });
        }
        pass.set_bind_group(0, &gpu.camera_group, &[]);
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_vertex_buffer(
            1,
            gpu.stroke_instances
                .as_ref()
                .expect("stroke buffer")
                .slice(..),
        );
        pass.draw(0..*count, draw.instance as u32..draw.instance as u32 + 1);
        1
    }

    pub fn is_active(&self) -> bool {
        !self.draws.is_empty() || self.paths.is_active()
    }

    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        resolve: Option<&wgpu::TextureView>,
        clear: wgpu::Color,
        sample_count: u32,
    ) -> DrawStats {
        let Some(gpu) = self.gpu.as_ref().filter(|_| self.is_active()) else {
            return DrawStats::default();
        };
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: color,
            depth_slice: None,
            resolve_target: resolve,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Noon retained mesh depth pass"),
            color_attachments: &attachments,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: if sample_count == 1 {
                    &gpu.depth
                } else {
                    gpu.depth_msaa
                        .get_or_init(|| depth(&gpu.device, gpu.viewport, 4))
                },
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let mut draw_calls = 0;
        if !self.draws.is_empty() {
            pass.set_pipeline(if sample_count == 1 {
                &gpu.pipeline
            } else {
                &gpu.pipeline_msaa
            });
            pass.set_bind_group(0, &gpu.camera_group, &[]);
            pass.set_vertex_buffer(1, gpu.instances.slice(..));
            for ((handle, cairo), ranges) in &self.mesh_instances {
                let mesh = &self.meshes[handle];
                if *cairo {
                    pass.set_pipeline(gpu.cairo.as_ref().expect("Cairo pipelines").select(
                        sample_count,
                        false,
                        false,
                    ));
                    pass.set_bind_group(1, mesh.cairo.as_ref().expect("Cairo appearance"), &[]);
                } else {
                    pass.set_pipeline(if sample_count == 1 {
                        &gpu.pipeline
                    } else {
                        &gpu.pipeline_msaa
                    });
                }
                pass.set_bind_group(0, &gpu.camera_group, &[]);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                for (start, end) in ranges.iter() {
                    pass.draw_indexed(0..mesh.index_count, 0, start as u32..end as u32);
                    draw_calls += 1;
                }
            }
        }
        for &index in &self.stroked_draws {
            let draw = &self.draws[&index];
            if !draw.transparent {
                draw_calls += self.encode_boundary(&mut pass, gpu, draw, sample_count);
            }
        }
        // Opaque world paths populate depth before translucent faces blend.
        draw_calls += self.paths.encode(&mut pass, sample_count);
        for &index in &self.transparent_order {
            let draw = &self.draws[&index];
            let mesh = &self.meshes[&draw.handle];
            if self.instances[draw.instance].color[3] > 0.0 {
                if is_cairo(draw.material) {
                    pass.set_pipeline(gpu.cairo.as_ref().expect("Cairo pipelines").select(
                        sample_count,
                        true,
                        false,
                    ));
                    pass.set_bind_group(1, mesh.cairo.as_ref().expect("Cairo appearance"), &[]);
                } else {
                    pass.set_pipeline(if sample_count == 1 {
                        &gpu.transparent_pipeline
                    } else {
                        &gpu.transparent_pipeline_msaa
                    });
                }
                pass.set_bind_group(0, &gpu.camera_group, &[]);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_vertex_buffer(1, gpu.instances.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(
                    0..mesh.index_count,
                    0,
                    draw.instance as u32..draw.instance as u32 + 1,
                );
                draw_calls += 1;
            }
            draw_calls += self.encode_boundary(&mut pass, gpu, draw, sample_count);
        }
        DrawStats {
            draw_calls,
            instances_drawn: self.draws.len() + self.paths.draw_count(),
        }
    }
}

/// Compact ordered half-open instance ranges. Membership changes touch only the
/// neighboring ranges; encoding iterates draws rather than every resident instance.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct InstanceRanges {
    ranges: BTreeMap<usize, usize>,
}

impl InstanceRanges {
    fn insert(&mut self, slot: usize) -> bool {
        let Some(mut end) = slot.checked_add(1) else {
            return false;
        };
        let mut start = slot;
        if let Some((&previous_start, &previous_end)) = self.ranges.range(..=slot).next_back() {
            if previous_end > slot {
                return false;
            }
            if previous_end == slot {
                start = previous_start;
                self.ranges.remove(&previous_start);
            }
        }
        if let Some((&next_start, &next_end)) = self.ranges.range(start..).next() {
            if next_start <= end {
                end = next_end;
                self.ranges.remove(&next_start);
            }
        }
        self.ranges.insert(start, end);
        true
    }

    fn remove(&mut self, slot: usize) -> bool {
        let Some((&start, &end)) = self.ranges.range(..=slot).next_back() else {
            return false;
        };
        if slot >= end {
            return false;
        }
        self.ranges.remove(&start);
        if start < slot {
            self.ranges.insert(start, slot);
        }
        if slot + 1 < end {
            self.ranges.insert(slot + 1, end);
        }
        true
    }

    fn iter(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.ranges.iter().map(|(&start, &end)| (start, end))
    }
}

fn lower_matrix(values: [f64; 16]) -> Option<[f32; 16]> {
    let result = values.map(|value| value as f32);
    result
        .iter()
        .all(|value| value.is_finite())
        .then_some(result)
}
fn is_cairo(material: SemanticSpatialMaterial) -> bool {
    material == SemanticSpatialMaterial::CairoSurface
}

fn instance(
    world: SemanticWorldTransform3D,
    color: [f32; 4],
    material: SemanticSpatialMaterial,
) -> Option<Instance> {
    let matrix = lower_matrix(world.world_matrix()?)?;
    let mut normals = [[0.0; 4]; 3];
    for (i, scale) in [world.scale.x, world.scale.y, world.scale.z]
        .into_iter()
        .enumerate()
    {
        let mut axis = [0.; 3];
        axis[i] = 1.;
        let axis = world
            .rotation
            .rotate_vector(SemanticVec3::new(axis[0], axis[1], axis[2]))?;
        // A singular transform has no normal inverse. Unlit meshes do not read
        // this attribute, so use a zero reciprocal for collapsed axes; PointLit
        // poses are rejected before staging and therefore always have a true
        // inverse-transpose normal basis.
        let reciprocal = if material != SemanticSpatialMaterial::PointLit || scale == 0. {
            0.
        } else {
            1. / scale
        };
        normals[i] = [
            (axis.x * reciprocal) as f32,
            (axis.y * reciprocal) as f32,
            (axis.z * reciprocal) as f32,
            if material == SemanticSpatialMaterial::PointLit {
                1.0
            } else {
                0.0
            },
        ];
    }
    normals
        .iter()
        .flatten()
        .chain(color.iter())
        .all(|value| value.is_finite())
        .then_some(Instance {
            world: matrix,
            normals,
            color,
        })
}
fn validate_point_lit_normals(
    mesh: &MeshResource,
    object_index: usize,
) -> Result<(), SpatialPrepareError> {
    if !mesh.has_usable_normals() {
        return Err(SpatialPrepareError::PointLitMeshNeedsNormals(object_index));
    }
    Ok(())
}
fn lower_vertices(mesh: &MeshResource) -> Result<Vec<Vertex>, SpatialPrepareError> {
    mesh.positions()
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let normal = mesh.normals().map_or(SemanticVec3::ZERO, |normals| {
                normalize_normal(normals[index])
            });
            let vertex = Vertex {
                position: [point.x as f32, point.y as f32, point.z as f32],
                normal: [normal.x as f32, normal.y as f32, normal.z as f32],
            };
            vertex
                .position
                .iter()
                .chain(vertex.normal.iter())
                .all(|value| value.is_finite())
                .then_some(vertex)
                .ok_or(SpatialPrepareError::UnrepresentableVertex)
        })
        .collect()
}
fn normalize_normal(value: SemanticVec3) -> SemanticVec3 {
    let largest = value.x.abs().max(value.y.abs()).max(value.z.abs());
    if largest == 0.0 || !largest.is_finite() {
        return SemanticVec3::ZERO;
    }
    let scaled = SemanticVec3::new(value.x / largest, value.y / largest, value.z / largest);
    let length = scaled.x.hypot(scaled.y).hypot(scaled.z);
    SemanticVec3::new(scaled.x / length, scaled.y / length, scaled.z / length)
}

impl GpuState {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        viewport: [u32; 2],
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon mesh camera layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let camera = create_buffer_with_data(
            device,
            queue,
            Some("Noon effective 3D camera"),
            bytemuck::cast_slice(&[0.0_f32; 16]),
            wgpu::BufferUsages::UNIFORM,
        );
        let light = create_buffer_with_data(
            device,
            queue,
            Some("Noon effective spatial point light"),
            bytemuck::bytes_of(&LightUniform::default()),
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let boundary_metrics = create_buffer_with_data(
            device,
            queue,
            Some("Noon mesh boundary viewport"),
            bytemuck::cast_slice(&[1.0_f32; 4]),
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Noon mesh camera"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: boundary_metrics.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: light.as_entire_binding(),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon mesh pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Noon retained mesh shader"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("mesh.wgsl"),
                    "\n",
                    include_str!("spatial_math.wgsl")
                )
                .into(),
            ),
        });
        let pipeline_msaa = pipeline(
            device,
            &pipeline_layout,
            &shader,
            format,
            4,
            false,
            PipelineKind::Mesh,
        );
        let opaque_pipeline = pipeline(
            device,
            &pipeline_layout,
            &shader,
            format,
            1,
            false,
            PipelineKind::Mesh,
        );
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Noon empty mesh instances"),
            size: 4,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            device: device.clone(),
            camera_layout: layout.clone(),
            format,
            cairo: None,
            viewport,
            pipeline: opaque_pipeline,
            pipeline_msaa,
            transparent_pipeline: pipeline(
                device,
                &pipeline_layout,
                &shader,
                format,
                1,
                true,
                PipelineKind::Mesh,
            ),
            transparent_pipeline_msaa: pipeline(
                device,
                &pipeline_layout,
                &shader,
                format,
                4,
                true,
                PipelineKind::Mesh,
            ),
            boundary_pipeline: pipeline(
                device,
                &pipeline_layout,
                &shader,
                format,
                1,
                true,
                PipelineKind::Boundary,
            ),
            boundary_pipeline_msaa: pipeline(
                device,
                &pipeline_layout,
                &shader,
                format,
                4,
                true,
                PipelineKind::Boundary,
            ),
            boundary_metrics,
            boundary_metrics_value: None,
            stroke_instances: None,
            stroke_capacity: 0,
            camera,
            camera_group,
            light,
            light_value: None,
            instances,
            instance_capacity: 0,
            depth: depth(device, viewport, 1),
            depth_msaa: OnceLock::new(),
        }
    }
    fn resize(&mut self, device: &wgpu::Device, viewport: [u32; 2]) {
        self.viewport = viewport;
        self.depth = depth(device, viewport, 1);
        self.depth_msaa.take();
    }
}
fn depth(device: &wgpu::Device, viewport: [u32; 2], count: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("Noon mesh viewport depth"),
            size: wgpu::Extent3d {
                width: viewport[0],
                height: viewport[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}
#[derive(Clone, Copy)]
enum PipelineKind {
    Mesh,
    Boundary,
    CairoMesh,
    CairoBoundary,
}

fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    count: u32,
    transparent: bool,
    kind: PipelineKind,
) -> wgpu::RenderPipeline {
    let boundary = matches!(kind, PipelineKind::Boundary | PipelineKind::CairoBoundary);
    let cairo = matches!(kind, PipelineKind::CairoMesh | PipelineKind::CairoBoundary);
    const VERTEX: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
    const EDGE: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 10 => Float32x2];
    const INSTANCE: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![2 => Float32x4, 3 => Float32x4, 4 => Float32x4,
        5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4, 9 => Float32x4];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Noon retained opaque mesh pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(match kind {
                PipelineKind::Mesh => "vs_main",
                PipelineKind::Boundary => "vs_boundary",
                PipelineKind::CairoMesh => "vs_cairo",
                PipelineKind::CairoBoundary => "vs_cairo_boundary",
            }),
            compilation_options: Default::default(),
            buffers: &[
                Some(wgpu::VertexBufferLayout {
                    array_stride: if boundary {
                        size_of::<boundary::EdgeVertex>()
                    } else {
                        size_of::<Vertex>()
                    } as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: if boundary { &EDGE } else { &VERTEX },
                }),
                Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &INSTANCE,
                }),
            ],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(if cairo {
                "fs_cairo"
            } else if boundary {
                "fs_boundary"
            } else {
                "fs_main"
            }),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: transparent.then_some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24Plus,
            depth_write_enabled: Some(!transparent),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: if boundary {
                wgpu::DepthBiasState {
                    constant: -1,
                    slope_scale: 0.0,
                    clamp: 0.0,
                }
            } else {
                Default::default()
            },
        }),
        multisample: wgpu::MultisampleState {
            count,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(test)]
mod tests {
    use super::{instance, validate_point_lit_normals, InstanceRanges, SpatialPrepareError};
    use noon_core::{
        MeshResource, SemanticRotation3D, SemanticSpatialMaterial, SemanticVec3,
        SemanticWorldTransform3D,
    };

    #[test]
    fn contiguous_membership_stays_one_range_and_holes_split_and_rejoin_locally() {
        let mut ranges = InstanceRanges::default();
        for slot in 0..600 {
            assert!(ranges.insert(slot));
        }
        assert_eq!(ranges.iter().collect::<Vec<_>>(), vec![(0, 600)]);
        assert!(!ranges.insert(299));

        assert!(ranges.remove(299));
        assert_eq!(
            ranges.iter().collect::<Vec<_>>(),
            vec![(0, 299), (300, 600)]
        );
        assert!(!ranges.remove(299));

        assert!(ranges.remove(0));
        assert!(ranges.remove(599));
        assert_eq!(
            ranges.iter().collect::<Vec<_>>(),
            vec![(1, 299), (300, 599)]
        );

        // Reusing the interior hole bridges two neighboring spans; reusing both
        // released boundary slots restores one compact draw range.
        assert!(ranges.insert(299));
        assert_eq!(ranges.iter().collect::<Vec<_>>(), vec![(1, 599)]);
        assert!(ranges.insert(0));
        assert!(ranges.insert(599));
        assert_eq!(ranges.iter().collect::<Vec<_>>(), vec![(0, 600)]);
    }

    #[test]
    fn point_lit_instances_use_inverse_transpose_normals_and_validated_sources() {
        let world = SemanticWorldTransform3D::new(
            SemanticVec3::ZERO,
            SemanticRotation3D::IDENTITY,
            SemanticVec3::new(2.0, 1.0, 0.5),
        )
        .unwrap();
        let instance = instance(
            world,
            [0.5, 0.25, 0.0, 1.0],
            SemanticSpatialMaterial::PointLit,
        )
        .unwrap();
        assert_eq!(instance.normals[0], [0.5, 0.0, 0.0, 1.0]);
        assert_eq!(instance.normals[1], [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(instance.normals[2], [0.0, 0.0, 2.0, 1.0]);

        let without_normals = MeshResource::new(
            vec![
                SemanticVec3::ZERO,
                SemanticVec3::new(1.0, 0.0, 0.0),
                SemanticVec3::new(0.0, 1.0, 0.0),
            ],
            None,
            vec![0, 1, 2],
        )
        .unwrap();
        assert_eq!(
            validate_point_lit_normals(&without_normals, 3),
            Err(SpatialPrepareError::PointLitMeshNeedsNormals(3))
        );
    }
}
