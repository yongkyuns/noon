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
    padding2: [f32; 2],
}

#[derive(Clone, Debug)]
pub(super) struct PreparedImageObject {
    pub object: ObjectId,
    pub content: RasterImageContentRef,
    pub uniform: ImageUniform,
    // Cloning this small immutable resource shares its Arc<[u8]>; never copy pixels
    // into instance rows or per-frame publications.
    pub resource: RasterImageResource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RasterImagePrepareError {
    MissingResource(RasterImageResourceHandle),
    DimensionMismatch(RasterImageResourceHandle),
    TextureLimit { width: u32, height: u32, limit: u32 },
    UnsupportedReveal(ObjectId),
    InvalidPresentation(ObjectId),
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
        }
    }
}
impl std::error::Error for RasterImagePrepareError {}

/// An unpublished sparse change set. All fallible resource checks finish before
/// this can replace renderer rows. A failed text/geometry preparation drops it.
pub(super) struct ImagePreparation {
    reset: bool,
    updates: Vec<(usize, Option<PreparedImageObject>)>,
}

#[derive(Debug)]
pub(super) struct RasterImageFramePreparer {
    pub owner: Arc<()>,
    pub generation: u64,
    pub base_generation: u64,
    pub objects: HashMap<usize, PreparedImageObject>,
    pub dirty: Vec<usize>,
    pub removed: Vec<usize>,
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
        texture_limit: u32,
    ) -> Result<ImagePreparation, RasterImagePrepareError> {
        let reset = changes.is_all() || !self.initialized;
        // An unsuccessful preparation cannot be reused by a later empty delta.
        self.initialized = false;
        let mut updates = Vec::new();
        let mut stage_slot = |index: usize| -> Result<(), RasterImagePrepareError> {
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
            let opacity = object.style.opacity
                * object.appearance
                * object.style.fill.map_or(1.0, |fill| fill.alpha);
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
                padding2: [0.0; 2],
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
                    row.object == object.id && row.content == content && row.uniform == uniform
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
            // Stable frame slots and the runtime's exact structural/object journal
            // allow add/remove/transform changes without scanning unrelated images.
            for &index in changes.object_indices() {
                stage_slot(index)?;
            }
        }
        Ok(ImagePreparation { reset, updates })
    }

    pub fn commit(&mut self, preparation: ImagePreparation) {
        self.initialized = true;
        if !preparation.reset && preparation.updates.is_empty() {
            return;
        }
        self.base_generation = self.generation;
        self.generation = self
            .generation
            .checked_add(1)
            .expect("image preparation generation exhausted");
        self.dirty.clear();
        self.removed.clear();
        if preparation.reset {
            self.removed.extend(self.objects.keys().copied());
            self.objects.clear();
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
    }
}
