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
    pub const fn new(resource: RasterImageResourceHandle) -> Self {
        Self {
            resource,
            sampling: RasterImageSampling::Linear,
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

/// Sampling is semantic because it can change visible pixels under scaling.
/// Keep the first tranche deliberately small and deterministic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RasterImageSampling {
    Nearest,
    #[default]
    Linear,
}
