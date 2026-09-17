use crate::RasterImageResourceHandle;

/// Renderer-independent semantic reference to immutable raster pixels.
///
/// Pixel storage lives in the store-owned `RasterImageResourceArena`; semantic
/// objects retain only this resource reference and visual sampling semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticImageContent {
    resource: RasterImageResourceHandle,
    sampling: RasterImageSampling,
}

impl SemanticImageContent {
    /// Immutable image color channels support alpha and affine edits, but not
    /// recoloring, strokes, or patterns.
    pub fn supports_style(style: &crate::SemanticStyle) -> bool {
        matches!(&style.fill, Some(crate::SemanticPaint::Solid(color))
            if color.red == 1.0 && color.green == 1.0 && color.blue == 1.0)
            && style.stroke.is_none()
            && style.stroke_width == 0.0
    }

    pub fn supports_render_style(style: &crate::Style) -> bool {
        style
            .fill
            .is_some_and(|color| color.red == 1.0 && color.green == 1.0 && color.blue == 1.0)
            && style.stroke.is_none()
            && style.stroke_width == 0.0
    }

    pub const fn new(resource: RasterImageResourceHandle) -> Self {
        Self {
            resource,
            sampling: RasterImageSampling::Bicubic,
        }
    }

    pub const fn with_sampling(
        resource: RasterImageResourceHandle,
        sampling: RasterImageSampling,
    ) -> Self {
        Self { resource, sampling }
    }

    pub const fn resource(self) -> RasterImageResourceHandle {
        self.resource
    }
    pub const fn sampling(self) -> RasterImageSampling {
        self.sampling
    }

    pub(crate) fn remap_resource(&mut self, resource: RasterImageResourceHandle) {
        self.resource = resource;
    }
}

/// Sampling is semantic because scaling may change visible pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RasterImageSampling {
    Nearest,
    Linear,
    #[default]
    Bicubic,
}

/// Compact execution reference. Dimensions are derived once from the immutable
/// resource during lowering; pixels never enter per-frame object state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RasterImageContentRef {
    content: SemanticImageContent,
    width: u32,
    height: u32,
}

impl RasterImageContentRef {
    pub fn from_resource(
        content: SemanticImageContent,
        resource: &crate::RasterImageResource,
    ) -> Self {
        Self {
            content,
            width: resource.width(),
            height: resource.height(),
        }
    }
    pub const fn resource(self) -> RasterImageResourceHandle {
        self.content.resource()
    }
    pub const fn sampling(self) -> RasterImageSampling {
        self.content.sampling()
    }
    pub const fn width(self) -> u32 {
        self.width
    }
    pub const fn height(self) -> u32 {
        self.height
    }
    pub const fn with_sampling(self, sampling: RasterImageSampling) -> Self {
        Self {
            content: SemanticImageContent::with_sampling(self.resource(), sampling),
            ..self
        }
    }
    pub const fn semantic_content(self) -> SemanticImageContent {
        self.content
    }

    /// Pixels define an intrinsic quad centered on the local origin.
    pub fn local_bounds(self) -> crate::Rect {
        let half = crate::Vec2::new(self.width as f32 * 0.5, self.height as f32 * 0.5);
        crate::Rect::new(-half, half)
    }
}
