//! Retained raster operator for `glow-encoded-ldr-v1` (#1897, M1).
//!
//! This module consumes already captured encoded-premultiplied source pixels and
//! an optional geometric silhouette. It owns only disposable GPU derivations.
//! Capturing the right padded source, painter placement, semantic identity,
//! effective animation values and publication remain with the existing owners.
//! In particular this operator does not enable M0's guarded Scene execution.

use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use noon_core::{EffectParameterError, Glow, GlowSource};

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
pub struct EffectPixelBounds {
    pub min: [f64; 2],
    pub max: [f64; 2],
}

impl EffectPixelBounds {
    /// Project one filled, unstroked M1 circle/rectangle through the existing
    /// effective 2D camera. Object nonuniform scale deforms the source, NOT
    /// the Gaussian radius; that radius stays in output/world units.
    pub fn projected_analytic(
        geometry: &noon_core::GeometryRef,
        transform: noon_core::Transform2D,
        camera: super::Camera2D,
        viewport: [u32; 2],
    ) -> Result<Self, EffectPrepareError> {
        Self::projected_analytic_in_view(
            geometry,
            transform,
            camera,
            viewport,
            noon_core::Vec2::new(viewport[0] as f32, viewport[1] as f32),
        )
    }

    /// Validate the uniform *logical* view separately from its raster sampling.
    /// Bounds and Gaussian support still use the exact physical output pixels.
    pub(super) fn projected_analytic_in_view(
        geometry: &noon_core::GeometryRef,
        transform: noon_core::Transform2D,
        camera: super::Camera2D,
        viewport: [u32; 2],
        logical_viewport: noon_core::Vec2,
    ) -> Result<Self, EffectPrepareError> {
        Self::validate_planar_view(transform, camera, viewport, logical_viewport)?;
        let x_pixels_per_world = f64::from(viewport[0]) / f64::from(camera.world_size.x);
        let y_pixels_per_world = f64::from(viewport[1]) / f64::from(camera.world_size.y);
        let (sin, cos) = f64::from(transform.rotation).sin_cos();
        let sx = f64::from(transform.scale.x);
        let sy = f64::from(transform.scale.y);
        let (world_half_x, world_half_y) = match geometry {
            noon_core::GeometryRef::Circle { radius } if radius.is_finite() && *radius > 0.0 => {
                let radius = f64::from(*radius);
                (
                    radius * (sx * cos).hypot(sy * sin),
                    radius * (sx * sin).hypot(sy * cos),
                )
            }
            noon_core::GeometryRef::Rectangle { size }
                if size.x.is_finite() && size.y.is_finite() && size.x > 0.0 && size.y > 0.0 =>
            {
                let half_x = f64::from(size.x) * 0.5;
                let half_y = f64::from(size.y) * 0.5;
                (
                    (half_x * sx * cos).abs() + (half_y * sy * sin).abs(),
                    (half_x * sx * sin).abs() + (half_y * sy * cos).abs(),
                )
            }
            noon_core::GeometryRef::Circle { .. } | noon_core::GeometryRef::Rectangle { .. } => {
                return Err(EffectPrepareError::InvalidSourceBounds);
            }
            _ => return Err(EffectPrepareError::UnsupportedCapture),
        };
        let screen_x = (f64::from(transform.translation.x) - f64::from(camera.center.x))
            * x_pixels_per_world
            + f64::from(viewport[0]) * 0.5;
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
            return Err(EffectPrepareError::CaptureCoordinatesOutOfRange);
        }
        Ok(bounds)
    }
    pub(super) fn validate_planar_view(
        transform: noon_core::Transform2D,
        camera: super::Camera2D,
        viewport: [u32; 2],
        logical_viewport: noon_core::Vec2,
    ) -> Result<(), EffectPrepareError> {
        if viewport.contains(&0)
            || ![
                camera.center.x,
                camera.center.y,
                camera.world_size.x,
                camera.world_size.y,
                logical_viewport.x,
                logical_viewport.y,
                transform.translation.x,
                transform.translation.y,
                transform.scale.x,
                transform.scale.y,
                transform.rotation,
            ]
            .into_iter()
            .all(f32::is_finite)
            || camera.world_size.x <= 0.0
            || camera.world_size.y <= 0.0
            || logical_viewport.x <= 0.0
            || logical_viewport.y <= 0.0
            || transform.scale.x == 0.0
            || transform.scale.y == 0.0
        {
            return Err(EffectPrepareError::InvalidProjection);
        }
        let logical_x_per_world = f64::from(logical_viewport.x) / f64::from(camera.world_size.x);
        let logical_y_per_world = f64::from(logical_viewport.y) / f64::from(camera.world_size.y);
        let disparity = (logical_x_per_world - logical_y_per_world).abs()
            / logical_x_per_world.max(logical_y_per_world);
        if disparity > 1e-5 {
            return Err(EffectPrepareError::InvalidProjection);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectCaptureTile {
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

impl EffectCaptureTile {
    /// Check before any GPU allocation. Return None only for validated
    /// neutral contributions or tiles whose entire halo misses the output.
    pub fn prepare(
        bounds: EffectPixelBounds,
        viewport: [u32; 2],
        parameters: impl Into<EffectCaptureParameters>,
        maximum_texture_dimension: u32,
        memory_budget_bytes: u64,
    ) -> Result<Option<Self>, EffectPrepareError> {
        let parameters = parameters.into();
        Self::prepare_stack(
            bounds,
            viewport,
            &[parameters.definition],
            super::EffectStackParameters {
                output_height: parameters.output_height,
                world_view_height: parameters.world_view_height,
                scope_opacity: parameters.scope_opacity,
                scratch_budget_bytes: memory_budget_bytes,
            },
            maximum_texture_dimension,
            memory_budget_bytes,
        )
    }

    /// Plan the full cumulative finite support before capture or GPU mutation.
    /// Silhouette glow always starts from original geometry; every painted
    /// operator expands the preceding output. AA padding applies once.
    pub fn prepare_stack(
        bounds: EffectPixelBounds,
        viewport: [u32; 2],
        definitions: &[noon_core::EffectDefinition],
        parameters: super::EffectStackParameters,
        maximum_texture_dimension: u32,
        memory_budget_bytes: u64,
    ) -> Result<Option<Self>, EffectPrepareError> {
        Self::prepare_stack_input(
            bounds,
            viewport,
            definitions,
            parameters,
            maximum_texture_dimension,
            memory_budget_bytes,
            false,
        )
    }

    /// An ancestor filter can consume pixels that are outside the final view.
    /// This changes contribution clipping only, never output units or the camera.
    pub(super) fn prepare_stack_input(
        bounds: EffectPixelBounds,
        viewport: [u32; 2],
        definitions: &[noon_core::EffectDefinition],
        parameters: super::EffectStackParameters,
        maximum_texture_dimension: u32,
        memory_budget_bytes: u64,
        required_by_parent: bool,
    ) -> Result<Option<Self>, EffectPrepareError> {
        if definitions.len() > noon_core::MAX_EFFECT_STACK_LEN {
            return Err(EffectPrepareError::StackExceedsProfile);
        }
        if !parameters.scope_opacity.is_finite() || !(0.0..=1.0).contains(&parameters.scope_opacity)
        {
            return Err(EffectPrepareError::InvalidScopeOpacity);
        }
        noon_core::EffectRadius::Scene(0.0)
            .to_output_pixels(parameters.output_height, parameters.world_view_height)
            .map_err(EffectPrepareError::Parameter)?;
        if viewport.contains(&0) {
            return Err(EffectPrepareError::Parameter(
                EffectParameterError::InvalidView,
            ));
        }
        if !bounds.min.into_iter().chain(bounds.max).all(f64::is_finite)
            || (0..2).any(|axis| bounds.min[axis] >= bounds.max[axis])
        {
            return Err(EffectPrepareError::InvalidSourceBounds);
        }
        let original = EffectPixelBounds {
            min: bounds.min.map(|v| v - 1.0),
            max: bounds.max.map(|v| v + 1.0),
        };
        let mut extent = original;
        let mut support_radius = 0_u32;
        let mut active = false;
        let mut cost_per_pixel = 4_u64; // one RGBA8 source
        let mut silhouette = false;
        for &definition in definitions {
            let value = EffectCaptureParameters {
                definition,
                output_height: parameters.output_height,
                world_view_height: parameters.world_view_height,
                scope_opacity: parameters.scope_opacity,
            };
            let sigma = value.sigma_pixels()?;
            let offset = value.offset_pixels()?;
            let mask = definition
                .as_glow()
                .is_some_and(|glow| glow.source() == GlowSource::Silhouette);
            silhouette |= mask;
            // Include warm neutral storage in the conservative scope budget.
            cost_per_pixel += value.filter_bytes_per_pixel();
            if value.is_neutral_at_sigma(sigma) {
                continue;
            }
            active = true;
            let support = (3.0 * sigma).ceil() as u32;
            support_radius += support;
            let input = if mask { original } else { extent };
            for (axis, offset) in offset.into_iter().enumerate() {
                extent.min[axis] =
                    extent.min[axis].min(input.min[axis] + offset.min(0.0) - f64::from(support));
                extent.max[axis] =
                    extent.max[axis].max(input.max[axis] + offset.max(0.0) + f64::from(support));
            }
        }
        cost_per_pixel += if silhouette { 4 } else { 0 };
        if !active || parameters.scope_opacity == 0.0 {
            return Ok(None);
        }
        let mut start = [0_i64; 2];
        let mut end = [0_i64; 2];
        for axis in 0..2 {
            let left = extent.min[axis].floor();
            let right = extent.max[axis].ceil();
            if !left.is_finite()
                || !right.is_finite()
                || left < f64::from(i32::MIN)
                || right > f64::from(i32::MAX)
            {
                return Err(EffectPrepareError::CaptureCoordinatesOutOfRange);
            }
            // Match the output raster's 2x2 derivative-quad grid. Integer pixel
            // alignment alone is insufficient: shifting a capture by one pixel
            // changes SDF fwidth coverage even when geometry is unchanged.
            // Round outward (including negative origins) so support is retained.
            start[axis] = (left as i64).div_euclid(2) * 2;
            end[axis] = right as i64;
        }
        let visible_start = [start[0].max(0), start[1].max(0)];
        let visible_end = [
            end[0].min(i64::from(viewport[0])),
            end[1].min(i64::from(viewport[1])),
        ];
        if !required_by_parent && (0..2).any(|axis| visible_end[axis] <= visible_start[axis]) {
            return Ok(None);
        }
        let width = end[0] - start[0];
        let height = end[1] - start[1];
        if width <= 0
            || height <= 0
            || width > i64::from(maximum_texture_dimension)
            || height > i64::from(maximum_texture_dimension)
        {
            return Err(EffectPrepareError::ExtentExceedsDevice);
        }
        let size = [width as u32, height as u32];
        let bytes = u64::from(size[0])
            .checked_mul(u64::from(size[1]))
            .and_then(|pixels| pixels.checked_mul(cost_per_pixel))
            .ok_or(EffectPrepareError::ScratchBudgetExceeded)?;
        if bytes > memory_budget_bytes {
            return Err(EffectPrepareError::ScratchBudgetExceeded);
        }
        let mut tile = Self {
            origin: [start[0] as i32, start[1] as i32],
            size,
            viewport_origin: [0; 2],
            local_origin: [0; 2],
            visible_size: [0; 2],
            support_radius,
            capture_and_scratch_bytes: bytes,
        };
        tile.refresh_visible_region(viewport);
        Ok(Some(tile))
    }

    pub(super) fn refresh_visible_region(&mut self, viewport: [u32; 2]) {
        for (axis, viewport) in viewport.into_iter().enumerate() {
            let origin = i64::from(self.origin[axis]);
            let start = origin.clamp(0, i64::from(viewport));
            let end = (origin + i64::from(self.size[axis])).clamp(0, i64::from(viewport));
            self.viewport_origin[axis] = start as u32;
            self.local_origin[axis] = (start - origin).clamp(0, i64::from(self.size[axis])) as u32;
            self.visible_size[axis] = (end - start).max(0) as u32;
        }
    }
}

/// Common isolated capture footprint for the finite built-ins.
#[derive(Clone, Copy, Debug)]
pub struct EffectCaptureParameters {
    pub definition: noon_core::EffectDefinition,
    pub output_height: u32,
    pub world_view_height: f64,
    pub scope_opacity: f32,
}
impl EffectCaptureParameters {
    pub(super) fn is_neutral_at_sigma(self, sigma: f64) -> bool {
        self.definition.is_neutral()
            || (matches!(self.definition, noon_core::EffectDefinition::Blur(_)) && sigma == 0.0)
    }
    pub(super) fn sigma_pixels(self) -> Result<f64, EffectPrepareError> {
        let sigma = self
            .definition
            .radius()
            .to_output_pixels(self.output_height, self.world_view_height)
            .map_err(EffectPrepareError::Parameter)?;
        if sigma > MAX_SIGMA {
            return Err(EffectPrepareError::RadiusExceedsProfile);
        }
        if !self.scope_opacity.is_finite() || !(0.0..=1.0).contains(&self.scope_opacity) {
            return Err(EffectPrepareError::InvalidScopeOpacity);
        }
        Ok(sigma)
    }
    pub(super) fn offset_pixels(self) -> Result<[f64; 2], EffectPrepareError> {
        let Some(shadow) = self.definition.as_shadow() else {
            return Ok([0.0; 2]);
        };
        let [x, y] = shadow
            .offset()
            .to_output_pixels(self.output_height, self.world_view_height)
            .map_err(EffectPrepareError::Parameter)?;
        if [x, y]
            .into_iter()
            .any(|v| !(v as f32).is_finite() || (f64::from(v as f32) - v).abs() > 1.0 / 256.0)
        {
            return Err(EffectPrepareError::CaptureCoordinatesOutOfRange);
        }
        Ok([x, -y])
    }
    pub(crate) fn filter_bytes_per_pixel(self) -> u64 {
        match self.definition {
            noon_core::EffectDefinition::Glow(_) => 12,
            noon_core::EffectDefinition::Blur(_) => 28,
            noon_core::EffectDefinition::Shadow(_) => 32,
        }
    }
}
impl From<GlowParameters> for EffectCaptureParameters {
    fn from(value: GlowParameters) -> Self {
        Self {
            definition: value.definition.into(),
            output_height: value.output_height,
            world_view_height: value.world_view_height,
            scope_opacity: value.scope_opacity,
        }
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
pub enum EffectPrepareError {
    Blur(super::gaussian_blur::GaussianBlurPrepareError),
    Parameter(EffectParameterError),
    InvalidScopeOpacity,
    RadiusExceedsProfile,
    UnsupportedCapture,
    MissingSilhouette,
    InvalidSourceBounds,
    CaptureCoordinatesOutOfRange,
    InvalidProjection,
    PublicationMismatch,
    CaptureSizeMismatch,
    ExtentExceedsDevice,
    ScratchBudgetExceeded,
    StackExceedsProfile,
    RevisionExhausted,
}
impl std::fmt::Display for EffectPrepareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot prepare effect raster operator: {self:?}")
    }
}
impl std::error::Error for EffectPrepareError {}

/// Actual resource and work counters; no physical performance claim is implied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectRasterStats {
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
    fn validate(parameters: GlowParameters) -> Result<f64, EffectPrepareError> {
        EffectCaptureParameters::from(parameters).sigma_pixels()
    }

    fn prepare(
        parameters: GlowParameters,
        size: [u32; 2],
        cached: Option<(f64, &Self)>,
    ) -> Result<(f64, Self), EffectPrepareError> {
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
            self.resources
                .as_ref()
                .map(|resource| &resource.output.texture)
        } else {
            None
        }
    }

    pub fn output_view(&self) -> Option<&wgpu::TextureView> {
        if self.active {
            self.resources
                .as_ref()
                .map(|resource| &resource.output.view)
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
    ) -> Result<EffectRasterStats, EffectPrepareError> {
        let size = [capture.source.width(), capture.source.height()];
        GlowUniform::validate(parameters)?;
        let mut stats = EffectRasterStats::default();
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
                .ok_or(EffectPrepareError::MissingSilhouette)?,
        };
        validate_capture(mask)?;
        if size != [mask.width(), mask.height()] {
            return Err(EffectPrepareError::CaptureSizeMismatch);
        }
        let limit = device.limits().max_texture_dimension_2d;
        if size.contains(&0) || size.iter().any(|&value| value > limit) {
            return Err(EffectPrepareError::ExtentExceedsDevice);
        }
        let scratch_bytes = u64::from(size[0])
            .checked_mul(u64::from(size[1]))
            .and_then(|pixels| pixels.checked_mul(SCRATCH_BYTES_PER_PIXEL))
            .ok_or(EffectPrepareError::ScratchBudgetExceeded)?;
        if scratch_bytes > parameters.scratch_budget_bytes {
            return Err(EffectPrepareError::ScratchBudgetExceeded);
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
        let sources_changed = scope
            .resources
            .as_ref()
            .is_none_or(|resources| resources.source != *capture.source || resources.mask != *mask);
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
        let kernel_changed = scope
            .uniform
            .as_ref()
            .is_none_or(|old| !old.same_kernel(&uniform));
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
    ) -> EffectRasterStats {
        let mut stats = EffectRasterStats {
            scratch_bytes: scope.scratch_bytes(),
            ..EffectRasterStats::default()
        };
        if !scope.active {
            return stats;
        }
        let programs = self.programs.as_ref().expect("active scope programs");
        let resources = scope.resources.as_ref().expect("active scope resources");
        if scope.blur_dirty {
            encode_pass(
                encoder,
                &resources.horizontal.view,
                &programs.horizontal,
                &resources.horizontal_binding,
                "Noon glow horizontal mask",
            );
            encode_pass(
                encoder,
                &resources.vertical.view,
                &programs.vertical,
                &resources.vertical_binding,
                "Noon glow vertical mask",
            );
            stats.blur_passes = 2;
        }
        if scope.output_dirty {
            encode_pass(
                encoder,
                &resources.output.view,
                &programs.composite,
                &resources.composite_binding,
                "Noon glow source-over-halo",
            );
            stats.composite_passes = 1;
        }
        scope.blur_dirty = false;
        scope.output_dirty = false;
        stats
    }
}

pub(super) fn validate_capture(texture: &wgpu::Texture) -> Result<(), EffectPrepareError> {
    if !matches!(
        texture.format(),
        wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
    ) || texture.dimension() != wgpu::TextureDimension::D2
        || texture.depth_or_array_layers() != 1
        || texture.sample_count() != 1
        || !texture
            .usage()
            .contains(wgpu::TextureUsages::TEXTURE_BINDING)
    {
        return Err(EffectPrepareError::UnsupportedCapture);
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
        Self {
            blur_layout,
            composite_layout,
            horizontal,
            vertical,
            composite,
        }
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
        let composite_binding = binding(
            device,
            &self.composite_layout,
            &uniform,
            &[&source_view, &vertical.view],
        );
        Resources {
            size,
            horizontal,
            vertical,
            output,
            uniform,
            horizontal_binding,
            vertical_binding,
            composite_binding,
            source: source.clone(),
            mask: mask.clone(),
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
        resources.horizontal_binding =
            binding(device, &self.blur_layout, &resources.uniform, &[&mask_view]);
        resources.composite_binding = binding(
            device,
            &self.composite_layout,
            &resources.uniform,
            &[&source_view, &resources.vertical.view],
        );
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
    entries.push(wgpu::BindGroupEntry {
        binding: 0,
        resource: uniform.as_entire_binding(),
    });
    for (index, &view) in images.iter().enumerate() {
        entries.push(wgpu::BindGroupEntry {
            binding: index as u32 + 1,
            resource: wgpu::BindingResource::TextureView(view),
        });
    }
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Noon retained glow binding"),
        layout,
        entries: &entries,
    })
}

#[cfg(test)]
mod tests;
