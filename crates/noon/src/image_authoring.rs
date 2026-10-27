//! Immutable raster preparation and shared image construction.
//!
//! Decoding and RGBA normalization happen once at authoring time. The semantic
//! store owns admitted pixels; all later operations retain ordinary Mobject
//! identity, transforms, styles, membership, and execution publication.
use std::{cell::RefCell, rc::Rc, sync::Arc};

use noon_core::{
    RasterImageResource, RasterImageSampling, SemanticImageContent, SemanticMutationImpact,
    SemanticMutationTransaction, SemanticMutationTransactionResult, SemanticNodeCreation,
    SemanticNodeId, SemanticObjectState, SemanticStore, SemanticVec3,
};

use crate::{semantic_mobject::authoring_render_f64, AuthoringError, Mobject};

/// ManimCE's resolution-independent image sizing default (high-quality height).
pub const DEFAULT_IMAGE_SCALE_TO_RESOLUTION: f64 = 1080.0;

/// Inert normalized pixels and local image-fill values. Admission happens only
/// while publishing the owning shape's ordinary style transaction.
#[derive(Clone, Debug)]
pub struct ImagePaintOptions {
    pixels: RasterImageResource,
    mapping: noon_core::ImageMapping,
    sampling: RasterImageSampling,
}

impl ImagePaintOptions {
    pub fn rgba8(
        width: u32,
        height: u32,
        pixels: impl Into<Arc<[u8]>>,
    ) -> Result<Self, AuthoringError> {
        Ok(Self::from_resource(RasterImageResource::from_rgba8(
            width, height, pixels,
        )?))
    }
    pub fn from_resource(pixels: RasterImageResource) -> Self {
        Self {
            pixels,
            mapping: noon_core::ImageMapping::default(),
            sampling: RasterImageSampling::Linear,
        }
    }
    pub fn from_image(image: &Mobject) -> Result<Self, AuthoringError> {
        let content = image
            .state()?
            .content
            .image()
            .ok_or(AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::ImageContent,
            ))?;
        let store = image.integration_store().borrow();
        let resource = store
            .raster_image_resources()
            .get(content.resource())
            .ok_or(AuthoringError::MissingImageResource(content.resource()))?;
        let mut options = Self::from_resource(resource.clone());
        options.sampling = content.sampling();
        Ok(options)
    }
    pub const fn pixel_width(&self) -> u32 {
        self.pixels.width()
    }
    pub const fn pixel_height(&self) -> u32 {
        self.pixels.height()
    }
    pub const fn mapping(&self) -> noon_core::ImageMapping {
        self.mapping
    }
    pub const fn sampling(&self) -> RasterImageSampling {
        self.sampling
    }
    pub fn set_mapping(&mut self, mapping: noon_core::ImageMapping) {
        self.mapping = mapping;
    }
    pub fn set_sampling(&mut self, sampling: RasterImageSampling) {
        self.sampling = sampling;
    }
}

pub(crate) fn publish_image_fill(
    store: &mut SemanticStore,
    object: SemanticNodeId,
    state: SemanticObjectState,
    options: ImagePaintOptions,
    opacity: Option<f64>,
    publish: impl FnOnce(
        &mut SemanticStore,
        SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError>,
) -> Result<SemanticMutationTransactionResult, AuthoringError> {
    store.with_raster_image_rgba8(
        options.pixels.width(),
        options.pixels.height(),
        options.pixels.into_rgba8(),
        |store, handle| {
            let paint = noon_core::SemanticPaint::Image(std::sync::Arc::new(
                noon_core::ImagePaint::new(handle, options.mapping, options.sampling),
            ));
            let state = crate::semantic_mobject::prepare_fill_paint_value(
                store, object, state, paint, opacity,
            )?;
            let mut transaction = SemanticMutationTransaction::new();
            transaction.replace_style(object, state.style);
            publish(store, transaction)
        },
    )
}

pub(crate) fn prepare_image_fill_mapping(
    object: &Mobject,
    mapping: noon_core::ImageMapping,
    opacity: Option<f64>,
) -> Result<SemanticObjectState, AuthoringError> {
    object.validate()?;
    let state = object.state()?;
    let image = *state
        .style
        .fill
        .as_ref()
        .and_then(noon_core::SemanticPaint::as_image)
        .ok_or(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::ContinuousPaintProfile,
        ))?;
    let paint = noon_core::SemanticPaint::Image(std::sync::Arc::new(noon_core::ImagePaint::new(
        image.resource(),
        mapping,
        image.sampling(),
    )));
    crate::semantic_mobject::prepare_fill_paint_value(
        &object.integration_store().borrow(),
        object.node_id(),
        state,
        paint,
        opacity,
    )
}

/// Inert image constructor. It owns prepared pixels, but no semantic identity or
/// retained-resource admission until a Scene/Mobject/live construction succeeds.
#[derive(Clone, Debug)]
pub struct ImageMobjectOptions {
    pixels: RasterImageResource,
    scale_to_resolution: f64,
    frame_height: f64,
    display_height: Option<f64>,
    sampling: RasterImageSampling,
    opacity: f64,
    z_index: f64,
}

impl ImageMobjectOptions {
    /// Row-major, tightly packed, straight-alpha RGBA8 with the first row at top.
    pub fn rgba8(
        width: u32,
        height: u32,
        pixels: impl Into<Arc<[u8]>>,
    ) -> Result<Self, AuthoringError> {
        Ok(Self::from_resource(RasterImageResource::from_rgba8(
            width, height, pixels,
        )?))
    }

    pub(crate) fn from_resource(pixels: RasterImageResource) -> Self {
        Self {
            pixels,
            scale_to_resolution: DEFAULT_IMAGE_SCALE_TO_RESOLUTION,
            frame_height: f64::from(noon_core::DEFAULT_FRAME_HEIGHT),
            display_height: None,
            sampling: RasterImageSampling::Bicubic,
            opacity: 1.0,
            z_index: 0.0,
        }
    }

    pub const fn pixel_width(&self) -> u32 {
        self.pixels.width()
    }
    pub const fn pixel_height(&self) -> u32 {
        self.pixels.height()
    }
    pub const fn sampling(&self) -> RasterImageSampling {
        self.sampling
    }
    pub fn set_sampling(&mut self, sampling: RasterImageSampling) {
        self.sampling = sampling;
    }

    /// Choose the virtual raster resolution used for Manim-compatible sizing.
    /// Zero deliberately selects Manim's fixed three-scene-unit fallback.
    pub fn set_scale_to_resolution(&mut self, resolution: f64) -> Result<(), AuthoringError> {
        let resolution = authoring_render_f64("image scale_to_resolution", resolution)?;
        if resolution < 0.0 {
            return Err(AuthoringError::NonPositiveNumber {
                name: "image scale_to_resolution".into(),
                value: resolution,
            });
        }
        self.scale_to_resolution = resolution;
        Ok(())
    }

    pub fn set_frame_height(&mut self, height: f64) -> Result<(), AuthoringError> {
        self.frame_height = positive("image frame_height", height)?;
        Ok(())
    }

    /// Set displayed height while preserving intrinsic aspect ratio.
    pub fn set_height(&mut self, height: f64) -> Result<(), AuthoringError> {
        self.display_height = Some(positive("image height", height)?);
        Ok(())
    }

    pub fn set_opacity(&mut self, opacity: f64) -> Result<(), AuthoringError> {
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(AuthoringError::InvalidOpacity {
                name: "image opacity".into(),
                value: opacity,
            });
        }
        self.opacity = opacity;
        Ok(())
    }

    pub fn set_z_index(&mut self, z_index: f64) -> Result<(), AuthoringError> {
        if !z_index.is_finite() {
            return Err(AuthoringError::NonFiniteObjectState);
        }
        self.z_index = z_index;
        Ok(())
    }

    fn scale(&self) -> Result<f64, AuthoringError> {
        let height = self.display_height.unwrap_or_else(|| {
            if self.scale_to_resolution == 0.0 {
                3.0
            } else {
                f64::from(self.pixels.height()) / self.scale_to_resolution * self.frame_height
            }
        });
        let scale = positive("image scale", height / f64::from(self.pixels.height()))?;
        authoring_render_f64(
            "image displayed width",
            scale * f64::from(self.pixels.width()),
        )?;
        authoring_render_f64("image displayed height", height)?;
        Ok(scale)
    }
}

fn positive(name: &str, value: f64) -> Result<f64, AuthoringError> {
    let value = authoring_render_f64(name, value)?;
    if value <= 0.0 || (value as f32) == 0.0 {
        return Err(AuthoringError::NonPositiveNumber {
            name: name.into(),
            value,
        });
    }
    Ok(value)
}

/// The same admission boundary serves cold authoring and prepared live execution.
/// The callback must finish all fallible publication work before committing.
pub(crate) fn publish_image_creation(
    store: &mut SemanticStore,
    options: ImageMobjectOptions,
    publish: impl FnOnce(
        &mut SemanticStore,
        SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, AuthoringError>,
) -> Result<SemanticNodeId, AuthoringError> {
    let scale = options.scale()?;
    let width = options.pixels.width();
    let height = options.pixels.height();
    store.with_raster_image_rgba8(
        width,
        height,
        options.pixels.into_rgba8(),
        |store, handle| {
            let mut state = SemanticObjectState::new(SemanticImageContent::with_sampling(
                handle,
                options.sampling,
            ));
            state.transform.scale = SemanticVec3::new(scale, scale, 1.0);
            state.style.fill = Some(noon_core::SemanticPaint::Solid(noon_core::Color::WHITE));
            state.style.fill_opacity = options.opacity;
            state.style.stroke = None;
            state.style.stroke_width = 0.0;
            state.style.stroke_opacity = 0.0;
            state.set_z_index(options.z_index);
            let mut transaction = SemanticMutationTransaction::new();
            transaction.add_node(SemanticNodeCreation::object(state));
            let result = publish(store, transaction)?;
            let [SemanticMutationImpact::NodeAdded { node }] = result.impacts() else {
                unreachable!("one image creation publishes exactly one semantic node")
            };
            Ok(*node)
        },
    )
}

impl Mobject {
    pub fn set_fill_image(
        &mut self,
        options: ImagePaintOptions,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.validate()?;
        let state = self.state()?;
        publish_image_fill(
            &mut self.integration_store().borrow_mut(),
            self.node_id(),
            state,
            options,
            opacity,
            |store, transaction| transaction.apply(store).map_err(AuthoringError::from),
        )
        .map(|_| ())
    }

    /// Edit only checked UV values, reusing the existing immutable pixel resource.
    pub fn set_fill_image_mapping(
        &mut self,
        mapping: noon_core::ImageMapping,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        let state = prepare_image_fill_mapping(self, mapping, opacity)?;
        self.commit_state(state)
    }
    /// Cold-authoring image constructor. For an already-running Scene use
    /// `Scene::image` or `LiveSession::create_image`, which preserve revision coherence.
    pub fn from_image(
        store: Rc<RefCell<SemanticStore>>,
        options: ImageMobjectOptions,
    ) -> Result<Self, AuthoringError> {
        let node =
            publish_image_creation(&mut store.borrow_mut(), options, |store, transaction| {
                transaction.apply(store).map_err(AuthoringError::from)
            })?;
        Self::from_node(store, node)
    }

    pub fn image_dimensions(&self) -> Result<(u32, u32), AuthoringError> {
        let state = self.state()?;
        let image = state.content.image().ok_or(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::ImageContent,
        ))?;
        let store = self.integration_store().borrow();
        let resource = store
            .raster_image_resources()
            .get(image.resource())
            .ok_or(AuthoringError::MissingImageResource(image.resource()))?;
        Ok((resource.width(), resource.height()))
    }

    pub fn image_sampling(&self) -> Result<RasterImageSampling, AuthoringError> {
        Ok(self
            .state()?
            .content
            .image()
            .ok_or(AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::ImageContent,
            ))?
            .sampling())
    }

    /// Sampling is a semantic content change, but never changes pixel identity.
    pub fn set_image_sampling(
        &mut self,
        sampling: RasterImageSampling,
    ) -> Result<(), AuthoringError> {
        let mut state = self.state()?;
        let image = state.content.image().ok_or(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::ImageContent,
        ))?;
        state.content = SemanticImageContent::with_sampling(image.resource(), sampling).into();
        self.commit_state(state)
    }
}
