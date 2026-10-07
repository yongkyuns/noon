//! Retained raster operator for `glow-encoded-ldr-v1` (#1897, M1).
//!
//! This module consumes already captured encoded-premultiplied source pixels and
//! an optional geometric silhouette. It owns only disposable GPU derivations.
//! Capturing the right padded source, painter placement, semantic identity,
//! effective animation values and publication remain with the existing owners.
//! In particular this operator does not enable M0's guarded Scene execution.

use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use noon_core::{Glow, GlowParameterError, GlowSource};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const MAX_SIGMA: f64 = 64.0;
const WEIGHT_VECTORS: usize = 64; // 193 used coefficients; portable fixed uniform array
const SCRATCH_BYTES_PER_PIXEL: u64 = 12; // two RGB24 masks and composed RGBA8

/// One coherent, renderer-owned source capture, before final scope opacity.
///
/// Both images use encoded premultiplied RGBA8 (never an sRGB texture). Silhouette
/// alpha represents geometric coverage independent of paint alpha. The revision
/// is the caller's existing content revision, not a new semantic identity.
/// Increase it when either capture changes, including in-place rendering.
#[derive(Clone, Copy, Debug)]
pub struct GlowCapture<'a> {
    pub source: &'a wgpu::Texture,
    pub silhouette: Option<&'a wgpu::Texture>,
    pub revision: u64,
}


/** Conservative source capture tile in final physical output pixels.
 * The caller supplies projected shape bounds, including its painted stroke.
 * Negative origins are valid: offscreen geometry can contribute a halo.
 */
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlowPixelBounds {
    pub min: [f64; 2],
    pub max: [f64; 2],
}


impl GlowPixelBounds {
    /// Project one filled, unstroked M1 circle/rectangle through the existing
    /// effective 2D camera. Object nonuniform scale deforms the source, NOT
    /// the Gaussian radius; that radius stays in output/world units.
    pub fn projected_analytic(
        geometry: &noon_core::GeometryRef,
        transform: noon_core::Transform2D,
        camera: super::Camera2D,
        viewport: [u32; 2],
    ) -> Result<Self, GlowPrepareError> {
        if viewport.contains(&0)
            || ![
                camera.center.x,
                camera.center.y,
                camera.world_size.x,
                camera.world_size.y,
                transform.translation.x,
                transform.translation.y,
                transform.scale.x,
                transform.scale.y,
                transform.rotation,
            ].into_iter().all(f32::is_finite)
            || camera.world_size.x <= 0.0
            || camera.world_size.y <= 0.0
            || transform.scale.x == 0.0
            || transform.scale.y == 0.0
        {
            return Err(GlowPrepareError::InvalidProjection);
        }
        let x_pixels_per_world = f64::from(viewport[0]) / f64::from(camera.world_size.x);
        let y_pixels_per_world = f64::from(viewport[1]) / f64::from(camera.world_size.y);
        let disparity = (x_pixels_per_world - y_pixels_per_world).abs()
            / x_pixels_per_world.max(y_pixels_per_world);
        if disparity > 1e-5 {
            return Err(GlowPrepareError::InvalidProjection);
        }
        let (sin, cos) = f64::from(transform.rotation).sin_cos();
        let sx = f64::from(transform.scale.x);
        let sy = f64::from(transform.scale.y);
        let (world_half_x, world_half_y) = match geometry {
            noon_core::GeometryRef::Circle { radius } if radius.is_finite() && *radius > 0.0 => {
                let radius = f64::from(*radius);
                (radius * (sx * cos).hypot(sy * sin),
                 radius * (sx * sin).hypot(sy * cos))
            }
            noon_core::GeometryRef::Rectangle { size }
                if size.x.is_finite() && size.y.is_finite()
                    && size.x > 0.0 && size.y > 0.0 => {
                let half_x = f64::from(size.x) * 0.5;
                let half_y = f64::from(size.y) * 0.5;
                ((half_x * sx * cos).abs() + (half_y * sy * sin).abs(),
                 (half_x * sx * sin).abs() + (half_y * sy * cos).abs())
            }
            noon_core::GeometryRef::Circle { .. }
            | noon_core::GeometryRef::Rectangle { .. } => {
                return Err(GlowPrepareError::InvalidSourceBounds);
            }
            _ => return Err(GlowPrepareError::UnsupportedCapture),
        };
        let screen_x = (f64::from(transform.translation.x) - f64::from(camera.center.x))
            * x_pixels_per_world + f64::from(viewport[0]) * 0.5;
        let screen_y = f64::from(viewport[1]) * 0.5
            - (f64::from(transform.translation.y) - f64::from(camera.center.y))
                * y_pixels_per_world;
        let horizontal = world_half_x * x_pixels_per_world;
        let vertical = world_half_y * y_pixels_per_world;
        let bounds = Self {
            min: [screen_x - horizontal, screen_y - vertical],
            max: [screen_x + horizontal, screen_y + vertical],
        };
        if !bounds.min.into_iter().chain(bounds.max).all(f64::is_finite) {
            return Err(GlowPrepareError::CaptureCoordinatesOutOfRange);
        }
        Ok(bounds)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlowCaptureTile {
    pub origin: [i32; 2],
    pub size: [u32; 2],
    pub viewport_origin: [u32; 2],
    pub local_origin: [u32; 2],
    pub visible_size: [u32; 2],
    pub support_radius: u32,
    /// Conservative aggregate: source RGBA8 + 3 filter images RGBA8,
    /// plus one silhouette mask RGBA8 only in silhouette mode.
    pub capture_and_scratch_bytes: u64,
}

impl GlowCaptureTile {
    /// Check before any GPU allocation. Return None only for validated
    /// neutral contributions or tiles whose entire halo misses the output.
    pub fn prepare(
        bounds: GlowPixelBounds,
        viewport: [u32; 2],
        parameters: GlowParameters,
        maximum_texture_dimension: u32,
        memory_budget_bytes: u64,
    ) -> Result<Option<Self>, GlowPrepareError> {
        let sigma = GlowUniform::validate(parameters)?;
        if viewport.contains(&0) {
            return Err(GlowPrepareError::Parameter(GlowParameterError::InvalidView));
        }
        if !bounds.min.into_iter().chain(bounds.max).all(f64::is_finite)
            || (0..2).any(|axis| bounds.min[axis] >= bounds.max[axis])
        {
            return Err(GlowPrepareError::InvalidSourceBounds);
        }
        if parameters.definition.is_neutral() || parameters.scope_opacity == 0.0 {
            return Ok(None);
        }
        let support_radius = (3.0 * sigma).ceil() as u32;
        let padding = f64::from(support_radius) + 1.0; // analytic AA footprint
        let mut start = [0_i64; 2];
        let mut end = [0_i64; 2];
        for axis in 0..2 {
            let left = (bounds.min[axis] - padding).floor();
            let right = (bounds.max[axis] + padding).ceil();
            if !left.is_finite()
                || !right.is_finite()
                || left < f64::from(i32::MIN)
                || right > f64::from(i32::MAX)
            {
                return Err(GlowPrepareError::CaptureCoordinatesOutOfRange);
            }
            start[axis] = left as i64;
            end[axis] = right as i64;
        }
        let visible_start = [start[0].max(0), start[1].max(0)];
        let visible_end = [
            end[0].min(i64::from(viewport[0])),
            end[1].min(i64::from(viewport[1])),
        ];
        if (0..2).any(|axis| visible_end[axis] <= visible_start[axis]) {
            return Ok(None);
        }
        let width = end[0] - start[0];
        let height = end[1] - start[1];
        if width <= 0
            || height <= 0
            || width > i64::from(maximum_texture_dimension)
            || height > i64::from(maximum_texture_dimension)
        {
            return Err(GlowPrepareError::ExtentExceedsDevice);
        }
        let size = [width as u32, height as u32];
        let cost_per_pixel: u64 = if parameters.definition.source() == GlowSource::Silhouette {
            20
        } else {
            16
        };
        let bytes = u64::from(size[0])
            .checked_mul(u64::from(size[1]))
            .and_then(|pixels| pixels.checked_mul(cost_per_pixel))
            .ok_or(GlowPrepareError::ScratchBudgetExceeded)?;
        if bytes > memory_budget_bytes {
            return Err(GlowPrepareError::ScratchBudgetExceeded);
        }
        Ok(Some(Self {
            origin: [start[0] as i32, start[1] as i32],
            size,
            viewport_origin: [visible_start[0] as u32, visible_start[1] as u32],
            local_origin: [
                (visible_start[0] - start[0]) as u32,
                (visible_start[1] - start[1]) as u32,
            ],
            visible_size: [
                (visible_end[0] - visible_start[0]) as u32,
                (visible_end[1] - visible_start[1]) as u32,
            ],
            support_radius,
            capture_and_scratch_bytes: bytes,
        }))
    }
}

/// Effective values supplied by the normal renderer preparation boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlowParameters {
    pub definition: Glow,
    /// Final output height, not the height of the local padded capture.
    pub output_height: u32,
    /// Height of the supported uniform planar view in scene units.
    pub world_view_height: f64,
    pub scope_opacity: f32,
    /// Budget for this operator's three scratch images. Source-capture and other
    /// scope budgets must additionally be enforced by the enclosing pass planner.
    pub scratch_budget_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlowPrepareError {
    Parameter(GlowParameterError),
    InvalidScopeOpacity,
    RadiusExceedsProfile,
    UnsupportedCapture,
    MissingSilhouette,
    InvalidSourceBounds,
    CaptureCoordinatesOutOfRange,
    InvalidProjection,
    CaptureSizeMismatch,
    ExtentExceedsDevice,
    ScratchBudgetExceeded,
}
impl std::fmt::Display for GlowPrepareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot prepare glow raster operator: {self:?}")
    }
}
impl std::error::Error for GlowPrepareError {}

/// Actual resource and work counters; no physical performance claim is implied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlowRasterStats {
    pub pipeline_compiles: usize,
    pub kernel_builds: usize,
    pub texture_allocations: usize,
    pub buffer_allocations: usize,
    pub bind_group_creations: usize,
    pub bytes_uploaded: usize,
    pub scratch_bytes: u64,
    pub blur_passes: usize,
    pub composite_passes: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct GlowUniform {
    size: [u32; 2],
    radius: u32,
    reserved: u32,
    tint: [f32; 4],
    control: [f32; 4],
    weights: [[f32; 4]; WEIGHT_VECTORS],
}

impl GlowUniform {
    fn validate(parameters: GlowParameters) -> Result<f64, GlowPrepareError> {
        let sigma = parameters
            .definition
            .radius()
            .to_output_pixels(parameters.output_height, parameters.world_view_height)
            .map_err(GlowPrepareError::Parameter)?;
        if sigma > MAX_SIGMA {
            return Err(GlowPrepareError::RadiusExceedsProfile);
        }
        if !parameters.scope_opacity.is_finite()
            || !(0.0..=1.0).contains(&parameters.scope_opacity)
        {
            return Err(GlowPrepareError::InvalidScopeOpacity);
        }
        Ok(sigma)
    }

    fn prepare(
        parameters: GlowParameters,
        size: [u32; 2],
        cached: Option<(f64, &Self)>,
    ) -> Result<(f64, Self), GlowPrepareError> {
        let sigma = Self::validate(parameters)?;
        let color = parameters.definition.color();
        let mut uniform = Self {
            size,
            radius: (3.0 * sigma).ceil() as u32,
            reserved: 0,
            tint: [color.red, color.green, color.blue, color.alpha],
            control: [
                parameters.definition.intensity() as f32,
                parameters.scope_opacity,
                0.0,
                0.0,
            ],
            weights: [[0.0; 4]; WEIGHT_VECTORS],
        };
        if let Some((old_sigma, old)) = cached {
            if old_sigma == sigma {
                uniform.weights = old.weights;
                return Ok((sigma, uniform));
            }
        }
        if sigma == 0.0 {
            uniform.weights[0][0] = 1.0;
            return Ok((sigma, uniform));
        }
        // Normalize the complete finite support once, never per edge pixel.
        // Dividing by sigma before squaring also handles positive subnormal sigma.
        let weight = |offset: u32| (-0.5 * (f64::from(offset) / sigma).powi(2)).exp();
        let normalization = 1.0 + 2.0 * (1..=uniform.radius).map(weight).sum::<f64>();
        for offset in 0..=uniform.radius {
            let index = offset as usize;
            uniform.weights[index / 4][index % 4] = (weight(offset) / normalization) as f32;
        }
        Ok((sigma, uniform))
    }

    fn same_kernel(&self, other: &Self) -> bool {
        self.size == other.size && self.radius == other.radius && self.weights == other.weights
    }
}

#[derive(Debug)]
struct Image {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}
impl Image {
    fn new(device: &wgpu::Device, size: [u32; 2], label: &'static str) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self { texture, view }
    }
}

#[derive(Debug)]
struct Resources {
    size: [u32; 2],
    horizontal: Image,
    vertical: Image,
    output: Image,
    uniform: wgpu::Buffer,
    horizontal_binding: wgpu::BindGroup,
    vertical_binding: wgpu::BindGroup,
    composite_binding: wgpu::BindGroup,
    source: wgpu::Texture,
    mask: wgpu::Texture,
}

/// Disposable resources for one isolated scope; owns no object or attachment ID.
/// Dropping or clearing it releases its GPU references. Multiple scopes share a
/// single `GlowFilter` and therefore the same three compiled programs.
#[derive(Debug, Default)]
pub struct GlowScope {
    resources: Option<Resources>,
    uniform: Option<GlowUniform>,
    source_revision: Option<u64>,
    sigma_pixels: Option<f64>,
    blur_dirty: bool,
    output_dirty: bool,
    active: bool,
}
impl GlowScope {
    /// The final encoded-premultiplied local image, after `encode` completes.
    /// Neutral treatment returns None: use the ordinary path, not a copied image.
    pub fn output(&self) -> Option<&wgpu::Texture> {
        if self.active {
            self.resources.as_ref().map(|resource| &resource.output.texture)
        } else {
            None
        }
    }

    pub fn output_view(&self) -> Option<&wgpu::TextureView> {
        if self.active {
            self.resources.as_ref().map(|resource| &resource.output.view)
        } else {
            None
        }
    }

    pub fn scratch_bytes(&self) -> u64 {
        self.resources.as_ref().map_or(0, |resource| {
            u64::from(resource.size[0]) * u64::from(resource.size[1]) * SCRATCH_BYTES_PER_PIXEL
        })
    }

    /// Re-encode retained work after an enclosing encoder was abandoned rather
    /// than submitted. Device replacement requires clearing the scope instead.
    pub fn invalidate(&mut self) {
        self.blur_dirty = true;
        self.output_dirty = true;
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug)]
struct Programs {
    blur_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    horizontal: wgpu::RenderPipeline,
    vertical: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
}

/// Renderer-local operator, lazy and shared across all glow scopes. No device,
/// queue submission, event loop, clock, semantic store or runtime is owned here.
#[derive(Debug, Default)]
pub struct GlowFilter {
    programs: Option<Programs>,
}
impl GlowFilter {
    /// Validate before changing the previous valid scope. Unchanged preparation
    /// performs no resource creation or upload. Intensity/tint/opacity changes
    /// upload 32 bytes and leave the cached Gaussian mask valid.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scope: &mut GlowScope,
        capture: GlowCapture<'_>,
        parameters: GlowParameters,
    ) -> Result<GlowRasterStats, GlowPrepareError> {
        let size = [capture.source.width(), capture.source.height()];
        GlowUniform::validate(parameters)?;
        let mut stats = GlowRasterStats::default();
        if parameters.definition.is_neutral() || parameters.scope_opacity == 0.0 {
            scope.active = false;
            stats.scratch_bytes = scope.scratch_bytes();
            return Ok(stats);
        }
        validate_capture(capture.source)?;
        let mask = match parameters.definition.source() {
            GlowSource::Painted => capture.source,
            GlowSource::Silhouette => capture
                .silhouette
                .ok_or(GlowPrepareError::MissingSilhouette)?,
        };
        validate_capture(mask)?;
        if size != [mask.width(), mask.height()] {
            return Err(GlowPrepareError::CaptureSizeMismatch);
        }
        let limit = device.limits().max_texture_dimension_2d;
        if size.contains(&0) || size.iter().any(|&value| value > limit) {
            return Err(GlowPrepareError::ExtentExceedsDevice);
        }
        let scratch_bytes = u64::from(size[0])
            .checked_mul(u64::from(size[1]))
            .and_then(|pixels| pixels.checked_mul(SCRATCH_BYTES_PER_PIXEL))
            .ok_or(GlowPrepareError::ScratchBudgetExceeded)?;
        if scratch_bytes > parameters.scratch_budget_bytes {
            return Err(GlowPrepareError::ScratchBudgetExceeded);
        }
        let cached = scope.sigma_pixels.zip(scope.uniform.as_ref());
        let (sigma, uniform) = GlowUniform::prepare(parameters, size, cached)?;
        // All fallible input admission precedes mutation/allocation.
        stats.kernel_builds = usize::from(scope.sigma_pixels != Some(sigma));
        let programs = self.programs.get_or_insert_with(|| {
            stats.pipeline_compiles = 3;
            Programs::new(device)
        });
        let recreate = scope
            .resources
            .as_ref()
            .is_none_or(|resources| resources.size != size);
        let sources_changed = scope.resources.as_ref().is_none_or(|resources| {
            resources.source != *capture.source || resources.mask != *mask
        });
        if recreate {
            scope.resources = Some(programs.resources(device, capture.source, mask, size));
            stats.texture_allocations = 3;
            stats.buffer_allocations = 1;
            stats.bind_group_creations = 3;
        } else if sources_changed {
            let resources = scope.resources.as_mut().expect("retained scope resources");
            programs.rebind_sources(device, resources, capture.source, mask);
            stats.bind_group_creations = 2;
        }
        let resources = scope.resources.as_ref().expect("prepared scope resources");
        let kernel_changed = scope.uniform.as_ref().is_none_or(|old| !old.same_kernel(&uniform));
        if recreate || kernel_changed {
            queue.write_buffer(&resources.uniform, 0, bytemuck::bytes_of(&uniform));
            stats.bytes_uploaded = size_of::<GlowUniform>();
        } else if scope.uniform != Some(uniform) {
            let bytes = bytemuck::bytes_of(&uniform);
            queue.write_buffer(&resources.uniform, 16, &bytes[16..48]);
            stats.bytes_uploaded = 32;
        }
        scope.blur_dirty |= recreate
            || sources_changed
            || kernel_changed
            || scope.source_revision != Some(capture.revision);
        scope.output_dirty |= scope.blur_dirty || scope.uniform != Some(uniform);
        scope.uniform = Some(uniform);
        scope.sigma_pixels = Some(sigma);
        scope.source_revision = Some(capture.revision);
        scope.active = true;
        stats.scratch_bytes = scratch_bytes;
        Ok(stats)
    }

    /// Encode into retained local scratch. The enclosing renderer captures the
    /// source and later places this result at its existing painter anchor. It
    /// also owns submission and output transfer. Calling twice without a dirty
    /// publication encodes no passes. Never use the output before submitting the
    /// encoder containing its latest work. If that encoder is abandoned, call
    /// `GlowScope::invalidate` before preparing/encoding again. Each scope must
    /// stay with its creating filter/device; clear it on renderer recreation.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scope: &mut GlowScope,
    ) -> GlowRasterStats {
        let mut stats = GlowRasterStats {
            scratch_bytes: scope.scratch_bytes(),
            ..GlowRasterStats::default()
        };
        if !scope.active {
            return stats;
        }
        let programs = self.programs.as_ref().expect("active scope programs");
        let resources = scope.resources.as_ref().expect("active scope resources");
        if scope.blur_dirty {
            encode_pass(encoder, &resources.horizontal.view, &programs.horizontal,
                &resources.horizontal_binding, "Noon glow horizontal mask");
            encode_pass(encoder, &resources.vertical.view, &programs.vertical,
                &resources.vertical_binding, "Noon glow vertical mask");
            stats.blur_passes = 2;
        }
        if scope.output_dirty {
            encode_pass(encoder, &resources.output.view, &programs.composite,
                &resources.composite_binding, "Noon glow source-over-halo");
            stats.composite_passes = 1;
        }
        scope.blur_dirty = false;
        scope.output_dirty = false;
        stats
    }
}

fn validate_capture(texture: &wgpu::Texture) -> Result<(), GlowPrepareError> {
    if !matches!(texture.format(), wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm)
        || texture.dimension() != wgpu::TextureDimension::D2
        || texture.depth_or_array_layers() != 1
        || texture.sample_count() != 1
        || !texture.usage().contains(wgpu::TextureUsages::TEXTURE_BINDING)
    {
        return Err(GlowPrepareError::UnsupportedCapture);
    }
    Ok(())
}

fn encode_pass(
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    binding: &wgpu::BindGroup,
    label: &'static str,
) {
    let attachments = [Some(wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            store: wgpu::StoreOp::Store,
        },
    })];
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &attachments,
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, binding, &[]);
    pass.draw(0..3, 0..1);
}

impl Programs {
    fn new(device: &wgpu::Device) -> Self {
        let uniform_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(size_of::<GlowUniform>() as u64),
            },
            count: None,
        };
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let blur_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon glow blur bindings"),
            entries: &[uniform_entry, texture_entry(1)],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon glow composite bindings"),
            entries: &[uniform_entry, texture_entry(1), texture_entry(2)],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("glow_filter.wgsl"));
        let pipeline = |layout: &wgpu::BindGroupLayout, entry: &'static str| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(entry),
                bind_group_layouts: &[Some(layout)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_fullscreen"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let horizontal = pipeline(&blur_layout, "fs_horizontal");
        let vertical = pipeline(&blur_layout, "fs_vertical");
        let composite = pipeline(&composite_layout, "fs_composite");
        Self { blur_layout, composite_layout, horizontal, vertical, composite }
    }

    fn resources(
        &self,
        device: &wgpu::Device,
        source: &wgpu::Texture,
        mask: &wgpu::Texture,
        size: [u32; 2],
    ) -> Resources {
        let horizontal = Image::new(device, size, "Noon glow horizontal RGB24");
        let vertical = Image::new(device, size, "Noon glow vertical RGB24");
        let output = Image::new(device, size, "Noon glow composed RGBA8");
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Noon glow uniform"),
            size: size_of::<GlowUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let source_view = source.create_view(&wgpu::TextureViewDescriptor::default());
        let mask_view = mask.create_view(&wgpu::TextureViewDescriptor::default());
        let horizontal_binding = binding(device, &self.blur_layout, &uniform, &[&mask_view]);
        let vertical_binding = binding(device, &self.blur_layout, &uniform, &[&horizontal.view]);
        let composite_binding = binding(device, &self.composite_layout, &uniform,
            &[&source_view, &vertical.view]);
        Resources {
            size, horizontal, vertical, output, uniform,
            horizontal_binding, vertical_binding, composite_binding,
            source: source.clone(), mask: mask.clone(),
        }
    }

    fn rebind_sources(
        &self,
        device: &wgpu::Device,
        resources: &mut Resources,
        source: &wgpu::Texture,
        mask: &wgpu::Texture,
    ) {
        let source_view = source.create_view(&wgpu::TextureViewDescriptor::default());
        let mask_view = mask.create_view(&wgpu::TextureViewDescriptor::default());
        resources.horizontal_binding = binding(device, &self.blur_layout,
            &resources.uniform, &[&mask_view]);
        resources.composite_binding = binding(device, &self.composite_layout,
            &resources.uniform, &[&source_view, &resources.vertical.view]);
        resources.source = source.clone();
        resources.mask = mask.clone();
    }
}

fn binding(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    images: &[&wgpu::TextureView],
) -> wgpu::BindGroup {
    let mut entries = Vec::with_capacity(images.len() + 1);
    entries.push(wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() });
    for (index, &view) in images.iter().enumerate() {
        entries.push(wgpu::BindGroupEntry {
            binding: index as u32 + 1,
            resource: wgpu::BindingResource::TextureView(view),
        });
    }
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Noon retained glow binding"), layout, entries: &entries,
    })
}

#[cfg(test)]
mod tests;
