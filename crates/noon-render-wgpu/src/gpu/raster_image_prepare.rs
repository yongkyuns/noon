//! Sparse, renderer-derived image rows. Pixels stay in immutable shared resources.
use std::{collections::HashMap, sync::Arc};

use bytemuck::{Pod, Zeroable};
use noon_core::{
    ObjectId, RasterImageContentRef, RasterImageResource, RasterImageResourceHandle,
    RasterImageResourceLookup, RasterImageSampling,
};
use noon_runtime::{FrameChanges, FrameState};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(super) struct ImageUniform {
    translation: [f32; 2],
    scale: [f32; 2],
    rotation: f32,
    opacity: f32,
    sampling: u32,
    padding: u32,
    dimensions: [f32; 2],
    crop_origin: [f32; 2],
}

impl ImageUniform {
    /// Remove final scope opacity for an isolated source. Intrinsic texture alpha
    /// is image coverage in both modes; silhouette omits only fill modulation.
    pub(super) fn captured(mut self, paint_opacity: f32, silhouette: bool) -> Self {
        self.opacity = if silhouette { 1.0 } else { paint_opacity };
        self
    }

    pub(super) fn transform(self) -> crate::PackedTransform {
        crate::PackedTransform {
            translation: self.translation,
            scale: self.scale,
            rotation: self.rotation,
            padding: 0.0,
        }
    }

    pub(super) fn with_capture_origin(mut self, origin: [i32; 2]) -> Self {
        self.crop_origin = [origin[0] as f32, origin[1] as f32];
        self
    }

    pub(super) fn inset(center: noon_core::Vec2, size: noon_core::Vec2, pixels: [u32; 2]) -> Self {
        Self {
            translation: [center.x, center.y],
            scale: [size.x / pixels[0] as f32, size.y / pixels[1] as f32],
            rotation: 0.0,
            opacity: 1.0,
            sampling: 2,
            padding: 0,
            dimensions: [pixels[0] as f32, pixels[1] as f32],
            crop_origin: [0.0; 2],
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct PreparedImageObject {
    pub object: ObjectId,
    pub content: RasterImageContentRef,
    pub uniform: ImageUniform,
    // Keep paint modulation separate from final scope opacity for isolated source
    // capture; recovering it by division would make opacity-only edits dirty.
    pub paint_opacity: f32,
    // This small immutable row shares its Arc<[u8]>; pixels never enter instance
    // rows or per-frame publications.
    pub resource: RasterImageResource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RasterImagePrepareError {
    MissingResource(RasterImageResourceHandle),
    DimensionMismatch(RasterImageResourceHandle),
    TextureLimit { width: u32, height: u32, limit: u32 },
    UnsupportedReveal(ObjectId),
    InvalidPresentation(ObjectId),
    InvalidPaint(ObjectId),
}

impl std::fmt::Display for RasterImagePrepareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingResource(h) => write!(
                f,
                "image resource {}:{}:{} is missing",
                h.arena,
                h.id.get(),
                h.version
            ),
            Self::DimensionMismatch(h) => write!(
                f,
                "image resource {} dimensions differ from its compiled reference",
                h.id.get()
            ),
            Self::TextureLimit {
                width,
                height,
                limit,
            } => write!(
                f,
                "image {width}x{height} exceeds the device texture limit {limit}"
            ),
            Self::UnsupportedReveal(id) => write!(
                f,
                "image {id:?} cannot use vector/glyph reveal or pixel morphing"
            ),
            Self::InvalidPresentation(id) => write!(f, "image {id:?} has an invalid presentation"),
            Self::InvalidPaint(id) => {
                write!(f, "object {id:?} has an invalid fill paint presentation")
            }
        }
    }
}
impl std::error::Error for RasterImagePrepareError {}

/// An unpublished sparse change set. All fallible resource checks finish before
/// this can replace renderer rows.
pub(super) struct ImagePreparation {
    reset: bool,
    updates: Vec<(usize, Option<PreparedImageObject>)>,
    paint_updates: Vec<(usize, Option<PreparedPaintImage>)>,
}

/// A texture dependency of the existing effective fill column. UV and opacity
/// changes stay in the paint uniform; this row changes only with pixel identity.
#[derive(Clone, Debug)]
pub(super) struct PreparedPaintImage {
    pub handle: RasterImageResourceHandle,
    pub resource: RasterImageResource,
}

#[derive(Debug)]
pub(super) struct RasterImageFramePreparer {
    pub owner: Arc<()>,
    pub generation: u64,
    pub base_generation: u64,
    pub objects: HashMap<usize, PreparedImageObject>,
    pub dirty: Vec<usize>,
    pub removed: Vec<usize>,
    pub paints: HashMap<usize, PreparedPaintImage>,
    pub paint_dirty: Vec<usize>,
    pub paint_removed: Vec<usize>,
    initialized: bool,
}

impl Default for RasterImageFramePreparer {
    fn default() -> Self {
        Self {
            owner: Arc::new(()),
            generation: 0,
            base_generation: 0,
            objects: HashMap::new(),
            dirty: Vec::new(),
            removed: Vec::new(),
            paints: HashMap::new(),
            paint_dirty: Vec::new(),
            paint_removed: Vec::new(),
            initialized: false,
        }
    }
}

impl RasterImageFramePreparer {
    pub fn stage(
        &mut self,
        frame: &FrameState,
        changes: &FrameChanges,
        resources: Option<&dyn RasterImageResourceLookup>,
        geometries: &(impl noon_core::GeometryResourceLookup + ?Sized),
        texture_limit: u32,
    ) -> Result<ImagePreparation, RasterImagePrepareError> {
        let reset = changes.is_all() || !self.initialized;
        // A failed preparation must never be reused by a later empty delta.
        self.initialized = false;
        let mut updates = Vec::new();
        let mut paint_updates = Vec::new();
        let mut stage_slot = |index: usize| -> Result<(), RasterImagePrepareError> {
            // Value-backed paints need the same atomic, resource-resolved
            // publication check as image fills, even though they own no pixels.
            if let Some(object) = frame.objects.get(index).filter(|_| frame.is_present(index)) {
                if let Some(noon_core::SemanticPaint::Hatch(hatch)) = object.fill_paint.as_deref() {
                    let geometry = object
                        .geometry()
                        .ok_or(RasterImagePrepareError::InvalidPaint(object.id))?;
                    let bounds =
                        super::retained_text::resolve_geometry_bounds(geometry, geometries)
                            .ok_or(RasterImagePrepareError::InvalidPaint(object.id))?;
                    noon_compile::validate_hatch_bounds(hatch, bounds)
                        .map_err(|_| RasterImagePrepareError::InvalidPaint(object.id))?;
                }
            }
            let paint = frame
                .objects
                .get(index)
                .filter(|_| frame.is_present(index))
                .and_then(|object| object.fill_paint.as_deref())
                .and_then(noon_core::SemanticPaint::as_image);
            if let Some(paint) = paint {
                let handle = paint.resource();
                let existing = self.paints.get(&index).filter(|row| row.handle == handle);
                let resource = resources
                    .and_then(|lookup| lookup.get(handle))
                    .or_else(|| existing.map(|row| &row.resource))
                    .ok_or(RasterImagePrepareError::MissingResource(handle))?;
                if resource.width() > texture_limit || resource.height() > texture_limit {
                    return Err(RasterImagePrepareError::TextureLimit {
                        width: resource.width(),
                        height: resource.height(),
                        limit: texture_limit,
                    });
                }
                let object = &frame.objects[index];
                let geometry = object
                    .geometry()
                    .ok_or(RasterImagePrepareError::InvalidPresentation(object.id))?;
                // Use the same versioned geometry lookup as ordinary retained
                // path preparation. External IDs have no inline local bounds.
                let bounds = super::retained_text::resolve_geometry_bounds(geometry, geometries)
                    .ok_or(RasterImagePrepareError::InvalidPresentation(object.id))?;
                let size = bounds.max - bounds.min;
                noon_compile::project_image_mapping(
                    paint.mapping(),
                    [f64::from(size.x), f64::from(size.y)],
                    [resource.width(), resource.height()],
                )
                .map_err(|_| RasterImagePrepareError::InvalidPresentation(object.id))?;
                if reset || existing.is_none() {
                    paint_updates.push((
                        index,
                        Some(PreparedPaintImage {
                            handle,
                            resource: resource.clone(),
                        }),
                    ));
                }
            } else if self.paints.contains_key(&index) {
                paint_updates.push((index, None));
            }
            let Some((object, content)) = frame
                .objects
                .get(index)
                .and_then(|object| object.content.image().map(|content| (object, content)))
                .filter(|_| frame.is_present(index))
            else {
                if self.objects.contains_key(&index) {
                    updates.push((index, None));
                }
                return Ok(());
            };
            if frame.reveal(index) != 1.0 || frame.morph(index) != 0.0 {
                return Err(RasterImagePrepareError::UnsupportedReveal(object.id));
            }
            let existing = self
                .objects
                .get(&index)
                .filter(|row| row.content.resource() == content.resource());
            let resource = match resources {
                Some(resources) => resources.get(content.resource()),
                None => existing.map(|row| &row.resource),
            }
            .ok_or(RasterImagePrepareError::MissingResource(content.resource()))?;
            if (resource.width(), resource.height()) != (content.width(), content.height()) {
                return Err(RasterImagePrepareError::DimensionMismatch(
                    content.resource(),
                ));
            }
            if content.width() > texture_limit || content.height() > texture_limit {
                return Err(RasterImagePrepareError::TextureLimit {
                    width: content.width(),
                    height: content.height(),
                    limit: texture_limit,
                });
            }
            if !noon_core::SemanticImageContent::supports_render_style(&object.style) {
                return Err(RasterImagePrepareError::InvalidPresentation(object.id));
            }
            let transform = frame.render_transform(index);
            let paint_opacity = object.style.fill.map_or(1.0, |fill| fill.alpha);
            let opacity = object.style.opacity * object.appearance * paint_opacity;
            let uniform = ImageUniform {
                translation: [transform.translation.x, transform.translation.y],
                scale: [transform.scale.x, transform.scale.y],
                rotation: transform.rotation,
                opacity,
                sampling: match content.sampling() {
                    RasterImageSampling::Nearest => 0,
                    RasterImageSampling::Linear => 1,
                    RasterImageSampling::Bicubic => 2,
                },
                padding: 0,
                dimensions: [content.width() as f32, content.height() as f32],
                crop_origin: [0.0; 2],
            };
            if !uniform
                .translation
                .iter()
                .chain(&uniform.scale)
                .all(|v| v.is_finite())
                || !uniform.rotation.is_finite()
                || !opacity.is_finite()
                || !(0.0..=1.0).contains(&opacity)
            {
                return Err(RasterImagePrepareError::InvalidPresentation(object.id));
            }
            if !reset
                && existing.is_some_and(|row| {
                    row.object == object.id
                        && row.content == content
                        && row.uniform == uniform
                        && row.paint_opacity == paint_opacity
                })
            {
                return Ok(());
            }
            updates.push((
                index,
                Some(PreparedImageObject {
                    object: object.id,
                    content,
                    uniform,
                    paint_opacity,
                    resource: resource.clone(),
                }),
            ));
            Ok(())
        };
        if reset {
            for index in 0..frame.objects.len() {
                stage_slot(index)?;
            }
        } else {
            // The runtime journal identifies only slots whose image residency can change.
            for &index in changes.object_indices() {
                stage_slot(index)?;
            }
        }
        Ok(ImagePreparation {
            reset,
            updates,
            paint_updates,
        })
    }

    pub fn commit(&mut self, preparation: ImagePreparation) {
        self.initialized = true;
        if !preparation.reset
            && preparation.updates.is_empty()
            && preparation.paint_updates.is_empty()
        {
            return;
        }
        self.base_generation = self.generation;
        self.generation = self
            .generation
            .checked_add(1)
            .expect("image preparation generation exhausted");
        self.dirty.clear();
        self.removed.clear();
        self.paint_dirty.clear();
        self.paint_removed.clear();
        if preparation.reset {
            self.removed.extend(self.objects.keys().copied());
            self.objects.clear();
            self.paint_removed.extend(self.paints.keys().copied());
            self.paints.clear();
        }
        for (index, row) in preparation.updates {
            match row {
                Some(row) => {
                    self.objects.insert(index, row);
                    self.dirty.push(index);
                }
                None => {
                    self.objects.remove(&index);
                    self.removed.push(index);
                }
            }
        }
        self.removed
            .retain(|index| !self.objects.contains_key(index));
        for (index, row) in preparation.paint_updates {
            if let Some(row) = row {
                self.paints.insert(index, row);
                self.paint_dirty.push(index);
            } else {
                self.paints.remove(&index);
                self.paint_removed.push(index);
            }
        }
        self.paint_removed
            .retain(|index| !self.paints.contains_key(index));
    }
}
