mod spatial_queries;
pub use spatial_queries::{PainterPrefixGeometryQuery, PainterPrefixGeometryQueryError};

use std::{collections::BTreeSet, sync::Arc};

use noon_core::{
    Color, FontResourceLookup, GeometryRef, GeometryResourceLookup, ObjectContentRef,
    PublicationContext, Rect, RetainedFamilyAnimationPlan, Style, TextResourceHandle,
    TextResourceLookup, Transform2D,
};

use crate::{FrameChanges, FrameState, RetainedPlannedFamilyFrame};

/// Borrow authored/static view metadata from the existing Runtime execution
/// projection, on the same publication lifetime as the effective frame.
pub trait ViewColorProfileLookup {
    fn view_color_profile(
        &self,
        object: noon_core::ObjectId,
    ) -> Option<noon_core::ViewColorProfile>;
}
impl ViewColorProfileLookup for crate::SceneInstance {
    fn view_color_profile(
        &self,
        object: noon_core::ObjectId,
    ) -> Option<noon_core::ViewColorProfile> {
        self.compiled
            .object_index(object)
            .and_then(|index| self.compiled.objects()[index as usize].view_color_profile)
    }
}
impl crate::SceneInstance {
    pub fn view_color_profile(
        &self,
        object: noon_core::ObjectId,
    ) -> Option<noon_core::ViewColorProfile> {
        ViewColorProfileLookup::view_color_profile(self, object)
    }
}

/// Borrow the publisher's existing identity index. Derived captures resolve
/// their local inputs without scanning the frame or retaining another index.
pub trait ObjectIndexLookup {
    fn object_index(&self, object: noon_core::ObjectId) -> Option<usize>;
    fn painter_rank(&self, index: usize) -> Option<usize>;
}

impl ObjectIndexLookup for crate::SceneInstance {
    fn painter_rank(&self, index: usize) -> Option<usize> {
        self.painter_rank(index).map(|rank| rank as usize)
    }

    fn object_index(&self, object: noon_core::ObjectId) -> Option<usize> {
        self.compiled
            .object_index(object)
            .map(|index| index as usize)
    }
}

/// One identity-free effective visual row for transient renderer presentation.
///
/// Unlike [`crate::FrameObjectState`], this type deliberately has no `ObjectId` and
/// therefore cannot enter the stable execution-slot domain. It can represent any
/// plan-local visual occurrence whose lifetime is bounded by execution/publication,
/// including plan-local copies and other transient effects. Content still
/// uses the same immutable retained resources as ordinary frame rows; there is no
/// second geometry/text world.
#[derive(Clone, Debug, PartialEq)]
pub struct DerivedDisplayObjectState {
    pub fill_paint: Option<Arc<noon_core::SemanticPaint>>,
    pub z_index: f64,
    pub content: ObjectContentRef,
    pub text_bounds: Option<Rect>,
    pub transform: Transform2D,
    pub style: Style,
    pub appearance: f32,
    pub presence: bool,
    pub reveal: f32,
    pub morph: f32,
    /// Optional execution-derived geometry override, mirroring `FrameState` without
    /// acquiring stable row identity.
    pub render_geometry: Option<Arc<GeometryRef>>,
    /// Optional execution-derived coordinate frame for `render_geometry`.
    pub render_transform: Option<Transform2D>,
}

impl DerivedDisplayObjectState {
    pub fn geometry(&self) -> Option<&GeometryRef> {
        self.content.geometry()
    }

    pub const fn text(&self) -> Option<TextResourceHandle> {
        self.content.text()
    }

    pub fn effective_render_geometry(&self) -> Option<&GeometryRef> {
        self.render_geometry.as_deref().or_else(|| self.geometry())
    }

    pub fn effective_render_transform(&self) -> Transform2D {
        self.render_transform.unwrap_or(self.transform)
    }
}

/// Which side of the real stable painter anchor contains a transient occurrence.
/// This is derived placement provenance, not a semantic layer or object identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TransientAnchorSide {
    Before,
    After,
}

/// One transient visual occurrence and its placement provenance.
///
/// `anchor_object_index` identifies an existing stable execution slot only for
/// painter placement and source-local invalidation. It is not the identity of this
/// occurrence. `occurrence_index` preserves deterministic order when multiple
/// transient occurrences share one source anchor.
#[derive(Clone, Debug, PartialEq)]
pub struct DerivedDisplayObject {
    anchor_object_index: u32,
    anchor_side: TransientAnchorSide,
    occurrence_index: u32,
    state: DerivedDisplayObjectState,
}

impl DerivedDisplayObject {
    pub const fn new(
        anchor_object_index: u32,
        occurrence_index: u32,
        state: DerivedDisplayObjectState,
    ) -> Self {
        Self {
            anchor_object_index,
            anchor_side: TransientAnchorSide::After,
            occurrence_index,
            state,
        }
    }

    pub const fn with_anchor_side(mut self, side: TransientAnchorSide) -> Self {
        self.anchor_side = side;
        self
    }

    pub const fn anchor_side(&self) -> TransientAnchorSide {
        self.anchor_side
    }

    pub const fn anchor_object_index(&self) -> u32 {
        self.anchor_object_index
    }

    pub const fn occurrence_index(&self) -> u32 {
        self.occurrence_index
    }

    pub const fn state(&self) -> &DerivedDisplayObjectState {
        &self.state
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DerivedDisplayPublicationError {
    AnchorOutOfRange {
        anchor_object_index: u32,
        object_count: usize,
    },
    AnchorNotPresent(u32),
    DuplicateOccurrence(u32),
    InvalidZIndex(u32),
    InvalidTransform(u32),
    InvalidStyle(u32),
    InvalidAppearance(u32),
    InvalidReveal(u32),
    InvalidMorph(u32),
}

impl std::fmt::Display for DerivedDisplayPublicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::AnchorOutOfRange {
                anchor_object_index,
                object_count,
            } => write!(
                formatter,
                "transient presentation anchor {anchor_object_index} is outside stable frame object count {object_count}"
            ),
            Self::AnchorNotPresent(index) => {
                write!(
                    formatter,
                    "transient presentation anchor {index} is not present in this frame"
                )
            }
            Self::DuplicateOccurrence(index) => {
                write!(
                    formatter,
                    "transient presentation occurrence index {index} is duplicated"
                )
            }
            Self::InvalidZIndex(index) => {
                write!(
                    formatter,
                    "transient presentation occurrence {index} has invalid z-index"
                )
            }
            Self::InvalidTransform(index) => {
                write!(
                    formatter,
                    "transient presentation occurrence {index} has invalid transform"
                )
            }
            Self::InvalidStyle(index) => {
                write!(
                    formatter,
                    "transient presentation occurrence {index} has invalid style"
                )
            }
            Self::InvalidAppearance(index) => {
                write!(
                    formatter,
                    "transient presentation occurrence {index} has invalid appearance"
                )
            }
            Self::InvalidReveal(index) => {
                write!(
                    formatter,
                    "transient presentation occurrence {index} has invalid reveal"
                )
            }
            Self::InvalidMorph(index) => {
                write!(
                    formatter,
                    "transient presentation occurrence {index} has invalid morph"
                )
            }
        }
    }
}

impl std::error::Error for DerivedDisplayPublicationError {}

/// Feature-neutral vocabulary for identity-free renderer presentation.
///
/// The `DerivedDisplay*` names remain temporarily as compatibility aliases for the
/// active B3 stack. New renderer/runtime consumers should use the
/// `TransientPresentation*` vocabulary so this lane does not encode its first
/// authoring feature into the architecture.
pub type TransientPresentationState = DerivedDisplayObjectState;
pub type TransientPresentationOccurrence = DerivedDisplayObject;
pub type TransientPresentationPublicationError = DerivedDisplayPublicationError;

/// One coherent borrowed runtime publication for renderer preparation.
///
/// The runtime creates this only while consuming accumulated changes. It keeps an
/// effective frame, its immutable projected resources, and their typed publication
/// context together without copying a second render world. Optional transient
/// presentation rows are borrowed execution output layered on that publication;
/// they never enter `FrameState` or stable execution storage.
pub struct RendererPublication<'a> {
    context: PublicationContext,
    frame: &'a FrameState,
    changes: FrameChanges,
    text_resources: &'a dyn TextResourceLookup,
    font_resources: &'a dyn FontResourceLookup,
    geometry_resources: &'a dyn GeometryResourceLookup,
    raster_image_resources: &'a dyn noon_core::RasterImageResourceLookup,
    family_animation_plans: &'a [RetainedFamilyAnimationPlan],
    active_family_animation_indices: &'a BTreeSet<usize>,
    planned_family_frame: RetainedPlannedFamilyFrame<'a>,
    painter_order: &'a [u32],
    derived_display_objects: &'a [DerivedDisplayObject],
    object_indices: &'a dyn ObjectIndexLookup,
    primary_view: Option<noon_core::ObjectId>,
    view_color_profiles: Option<&'a dyn ViewColorProfileLookup>,
    spatial_index: Option<&'a crate::ExecutionSpatialIndex>,
}

impl<'a> RendererPublication<'a> {
    /// Select the compiler-derived primary camera for this publication. The
    /// profile itself remains borrowed from the current execution revision.
    pub fn with_primary_view(mut self, camera: Option<noon_core::ObjectId>) -> Self {
        self.primary_view = camera;
        self
    }

    /// The working color profile of the selected authored view. Publications
    /// without a 2D view controller keep the ordinary compatibility profile.
    pub const fn primary_view(&self) -> Option<noon_core::ObjectId> {
        self.primary_view
    }

    /// Effective attachment values on the selected authored camera, borrowed
    /// from the ordinary frame row. No separate view-effects state is retained.
    pub fn view_effects(&self) -> Option<&noon_compile::CompiledEffectStack> {
        let camera = self.primary_view?;
        self.view_color_profile(camera)?;
        self.frame.objects[self.object_index(camera)?]
            .effects
            .as_deref()
    }

    pub fn color_profile(&self) -> noon_core::ViewColorProfile {
        self.primary_view
            .and_then(|camera| self.view_color_profile(camera))
            .unwrap_or_default()
    }

    /// Borrow static camera metadata from the publishing execution projection,
    /// including validated installed data at a genuine transport boundary.
    pub fn with_view_color_profiles(mut self, profiles: &'a dyn ViewColorProfileLookup) -> Self {
        self.view_color_profiles = Some(profiles);
        self
    }
    pub fn view_color_profile(
        &self,
        object: noon_core::ObjectId,
    ) -> Option<noon_core::ViewColorProfile> {
        self.object_index(object)?;
        self.view_color_profiles?.view_color_profile(object)
    }

    pub const fn context(&self) -> PublicationContext {
        self.context
    }

    pub const fn frame(&self) -> &FrameState {
        self.frame
    }

    pub fn object_index(&self, object: noon_core::ObjectId) -> Option<usize> {
        self.object_indices.object_index(object).filter(|&index| {
            self.frame
                .objects
                .get(index)
                .is_some_and(|row| row.id == object)
        })
    }

    pub fn painter_rank(&self, index: usize) -> Option<usize> {
        self.object_indices.painter_rank(index).filter(|&rank| {
            self.painter_order
                .get(rank)
                .copied()
                .map(|row| row as usize)
                == Some(index)
        })
    }

    /// Execution-owned broad-phase geometry bounds for this exact publication.
    /// Filters may additionally contribute outside these bounds; the renderer
    /// must expand that declared source closure before capturing a prefix.
    pub const fn spatial_index(&self) -> Option<&crate::ExecutionSpatialIndex> {
        self.spatial_index
    }

    pub const fn changes(&self) -> &FrameChanges {
        &self.changes
    }

    pub fn text_resources(&self) -> &dyn TextResourceLookup {
        self.text_resources
    }

    pub fn font_resources(&self) -> &dyn FontResourceLookup {
        self.font_resources
    }

    pub fn raster_image_resources(&self) -> &dyn noon_core::RasterImageResourceLookup {
        self.raster_image_resources
    }

    pub fn geometry_resources(&self) -> &dyn GeometryResourceLookup {
        self.geometry_resources
    }

    pub fn planned_family_frame(&self) -> RetainedPlannedFamilyFrame<'_> {
        self.planned_family_frame
    }

    pub fn family_animation_plans(&self) -> &[RetainedFamilyAnimationPlan] {
        self.family_animation_plans
    }

    pub fn active_family_animation_indices(&self) -> &BTreeSet<usize> {
        self.active_family_animation_indices
    }

    pub const fn painter_order(&self) -> &[u32] {
        self.painter_order
    }

    /// Identity-free transient occurrences attached to this exact publication.
    pub const fn transient_presentations(&self) -> &[TransientPresentationOccurrence] {
        self.derived_display_objects
    }

    /// Migration accessor for the currently stacked B3 work.
    /// New generic renderer/runtime code should use [`Self::transient_presentations`].
    pub const fn derived_display_objects(&self) -> &[DerivedDisplayObject] {
        self.derived_display_objects
    }

    /// Escalate an acquired redraw to a full renderer invalidation while retaining
    /// this publication's exact frame, resources, and revision context.
    pub fn invalidate_all(&mut self) {
        self.changes = FrameChanges::all();
    }
}

impl<'a> RendererPublication<'a> {
    /// Borrow the caller's synchronized execution index. The session or validated
    /// transport receiver must update it to this publication before borrowing it.
    /// This adds no renderer-owned object bounds or identity allocator.
    #[doc(hidden)]
    pub fn with_spatial_index(mut self, index: &'a crate::ExecutionSpatialIndex) -> Self {
        self.spatial_index = Some(index);
        self
    }

    /// Attach identity-free transient presentation rows to the exact publication
    /// that owns the effective stable frame they were derived from.
    ///
    /// Every anchor and value is checked before the borrowed slice is installed, so
    /// failure leaves the publication unchanged. Stable execution identity and
    /// retained resource ownership are never synthesized for these occurrences.
    pub fn with_transient_presentations(
        mut self,
        presentations: &'a [TransientPresentationOccurrence],
    ) -> Result<Self, TransientPresentationPublicationError> {
        validate_derived_display_objects(self.frame, presentations)?;
        self.derived_display_objects = presentations;
        Ok(self)
    }

    /// Migration entry point for the currently stacked B3 implementation. It
    /// delegates to the feature-neutral publication contract.
    pub fn with_derived_display_objects(
        self,
        derived_display_objects: &'a [DerivedDisplayObject],
    ) -> Result<Self, DerivedDisplayPublicationError> {
        self.with_transient_presentations(derived_display_objects)
    }

    /// Reconstitute the exact borrowed renderer publication at the genuine
    /// retained execution-worker transport boundary. The caller must have
    /// atomically validated and installed the typed transport envelope, resource
    /// identities, epoch, and painter order using the normal worker mirror.
    ///
    /// This borrows the worker's *existing* effective frame and resources. It is
    /// neither an authoring entry point nor a second execution engine or state.
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub fn from_validated_transport_frame(
        context: PublicationContext,
        planned_family_frame: RetainedPlannedFamilyFrame<'a>,
        changes: FrameChanges,
        text_resources: &'a dyn TextResourceLookup,
        font_resources: &'a dyn FontResourceLookup,
        geometry_resources: &'a dyn GeometryResourceLookup,
        raster_image_resources: &'a dyn noon_core::RasterImageResourceLookup,
        family_animation_plans: &'a [RetainedFamilyAnimationPlan],
        active_family_animation_indices: &'a BTreeSet<usize>,
        painter_order: &'a [u32],
        object_indices: &'a dyn ObjectIndexLookup,
    ) -> Self {
        let mut publication = Self::new(
            context,
            planned_family_frame.retained,
            changes,
            text_resources,
            font_resources,
            geometry_resources,
            raster_image_resources,
            family_animation_plans,
            active_family_animation_indices,
            painter_order,
            object_indices,
        );
        publication.planned_family_frame = planned_family_frame;
        publication
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        context: PublicationContext,
        frame: &'a FrameState,
        changes: FrameChanges,
        text_resources: &'a dyn TextResourceLookup,
        font_resources: &'a dyn FontResourceLookup,
        geometry_resources: &'a dyn GeometryResourceLookup,
        raster_image_resources: &'a dyn noon_core::RasterImageResourceLookup,
        family_animation_plans: &'a [RetainedFamilyAnimationPlan],
        active_family_animation_indices: &'a BTreeSet<usize>,
        painter_order: &'a [u32],
        object_indices: &'a dyn ObjectIndexLookup,
    ) -> Self {
        Self {
            context,
            frame,
            changes,
            text_resources,
            font_resources,
            geometry_resources,
            raster_image_resources,
            family_animation_plans,
            active_family_animation_indices,
            planned_family_frame: RetainedPlannedFamilyFrame {
                retained: frame,
                family_animations: &frame.family_animations,
                family_plan_indices: &frame.family_animation_plan_indices,
            },
            painter_order,
            derived_display_objects: &[],
            object_indices,
            primary_view: None,
            view_color_profiles: None,
            spatial_index: None,
        }
    }
}

fn validate_derived_display_objects(
    frame: &FrameState,
    objects: &[DerivedDisplayObject],
) -> Result<(), DerivedDisplayPublicationError> {
    let mut occurrences = BTreeSet::new();
    for object in objects {
        let anchor = object.anchor_object_index as usize;
        if anchor >= frame.objects.len() {
            return Err(DerivedDisplayPublicationError::AnchorOutOfRange {
                anchor_object_index: object.anchor_object_index,
                object_count: frame.objects.len(),
            });
        }
        if !frame.is_present(anchor) {
            return Err(DerivedDisplayPublicationError::AnchorNotPresent(
                object.anchor_object_index,
            ));
        }
        if !occurrences.insert(object.occurrence_index) {
            return Err(DerivedDisplayPublicationError::DuplicateOccurrence(
                object.occurrence_index,
            ));
        }
        let state = &object.state;
        if !state.z_index.is_finite() {
            return Err(DerivedDisplayPublicationError::InvalidZIndex(
                object.occurrence_index,
            ));
        }
        if !transform_is_finite(state.transform)
            || state
                .render_transform
                .is_some_and(|value| !transform_is_finite(value))
        {
            return Err(DerivedDisplayPublicationError::InvalidTransform(
                object.occurrence_index,
            ));
        }
        if !style_is_finite(state.style) {
            return Err(DerivedDisplayPublicationError::InvalidStyle(
                object.occurrence_index,
            ));
        }
        if !normalized(state.appearance) {
            return Err(DerivedDisplayPublicationError::InvalidAppearance(
                object.occurrence_index,
            ));
        }
        if !normalized(state.reveal) {
            return Err(DerivedDisplayPublicationError::InvalidReveal(
                object.occurrence_index,
            ));
        }
        if !normalized(state.morph) {
            return Err(DerivedDisplayPublicationError::InvalidMorph(
                object.occurrence_index,
            ));
        }
    }
    Ok(())
}

fn normalized(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn transform_is_finite(value: Transform2D) -> bool {
    value.translation.x.is_finite()
        && value.translation.y.is_finite()
        && value.rotation.is_finite()
        && value.scale.x.is_finite()
        && value.scale.y.is_finite()
}

fn style_is_finite(value: Style) -> bool {
    value.stroke_width.is_finite()
        && value.opacity.is_finite()
        && value.fill.is_none_or(color_is_finite)
        && value.stroke.is_none_or(color_is_finite)
}

fn color_is_finite(value: Color) -> bool {
    value.red.is_finite()
        && value.green.is_finite()
        && value.blue.is_finite()
        && value.alpha.is_finite()
}

#[cfg(test)]
mod derived_display_tests {
    use noon_core::{GeometryRef, ObjectId, Style, Vec2};

    use super::*;
    use crate::FrameObjectState;

    fn derived_state() -> DerivedDisplayObjectState {
        DerivedDisplayObjectState {
            fill_paint: None,
            z_index: 2.0,
            content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
            text_bounds: None,
            transform: Transform2D {
                translation: Vec2::new(1.0, 2.0),
                ..Transform2D::IDENTITY
            },
            style: Style::default(),
            appearance: 0.5,
            presence: true,
            reveal: 1.0,
            morph: 0.0,
            render_geometry: None,
            render_transform: None,
        }
    }

    fn one_object_frame() -> FrameState {
        FrameState {
            time: 0.0,
            objects: vec![FrameObjectState {
                fill_paint: None,
                draw_enabled: true,
                effects: None,
                id: ObjectId::new(1),
                z_index: 0.0,
                content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                text_bounds: None,
                transform: Transform2D::IDENTITY,
                spatial: None,
                style: Style::default(),
                appearance: 1.0,
            }],
            presences: vec![true],
            reveals: vec![1.0],
            morphs: vec![0.0],
            render_geometries: vec![None],
            render_transforms: vec![None],
            family_animations: vec![None],
            family_animation_plan_indices: vec![None],
        }
    }

    #[test]
    fn derived_display_row_reuses_retained_content_without_execution_identity() {
        let state = derived_state();

        assert!(matches!(
            state.effective_render_geometry(),
            Some(GeometryRef::Circle { radius }) if *radius == 1.0
        ));
        assert_eq!(state.effective_render_transform(), state.transform);
        assert_eq!(state.text(), None);
    }

    #[test]
    fn derived_display_occurrence_carries_only_existing_anchor_and_local_order() {
        let occurrence = DerivedDisplayObject::new(7, 2, derived_state());
        assert_eq!(occurrence.anchor_object_index(), 7);
        assert_eq!(occurrence.occurrence_index(), 2);
        assert_eq!(occurrence.state().appearance, 0.5);
    }

    #[test]
    fn transient_presentation_vocabulary_reuses_identity_free_occurrence_contract() {
        let occurrence: TransientPresentationOccurrence =
            TransientPresentationOccurrence::new(4, 9, derived_state());
        let state: &TransientPresentationState = occurrence.state();
        let no_error: Option<TransientPresentationPublicationError> = None;

        assert_eq!(occurrence.anchor_object_index(), 4);
        assert_eq!(occurrence.occurrence_index(), 9);
        assert_eq!(state.appearance, 0.5);
        assert!(no_error.is_none());
    }

    #[test]
    fn derived_display_preflight_rejects_anchor_and_value_failures_atomically() {
        let frame = one_object_frame();
        let valid = DerivedDisplayObject::new(0, 0, derived_state());
        assert_eq!(validate_derived_display_objects(&frame, &[valid]), Ok(()));

        let invalid_anchor = DerivedDisplayObject::new(1, 1, derived_state());
        assert!(matches!(
            validate_derived_display_objects(&frame, &[invalid_anchor]),
            Err(DerivedDisplayPublicationError::AnchorOutOfRange { .. })
        ));

        let mut invalid_state = derived_state();
        invalid_state.transform.rotation = f32::NAN;
        let invalid_value = DerivedDisplayObject::new(0, 2, invalid_state);
        assert_eq!(
            validate_derived_display_objects(&frame, &[invalid_value]),
            Err(DerivedDisplayPublicationError::InvalidTransform(2))
        );
    }

    #[test]
    fn derived_display_preflight_rejects_duplicate_occurrence_indices() {
        let frame = one_object_frame();
        let first = DerivedDisplayObject::new(0, 3, derived_state());
        let second = DerivedDisplayObject::new(0, 3, derived_state());
        assert_eq!(
            validate_derived_display_objects(&frame, &[first, second]),
            Err(DerivedDisplayPublicationError::DuplicateOccurrence(3))
        );
    }

    #[test]
    fn derived_display_overrides_stay_local_to_the_visual_row() {
        let override_transform = Transform2D {
            translation: Vec2::new(-3.0, 4.0),
            ..Transform2D::IDENTITY
        };
        let state = DerivedDisplayObjectState {
            fill_paint: None,
            z_index: 0.0,
            content: ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
            text_bounds: None,
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
            presence: true,
            reveal: 0.5,
            morph: 0.25,
            render_geometry: Some(Arc::new(GeometryRef::rectangle(3.0, 4.0))),
            render_transform: Some(override_transform),
        };

        assert!(matches!(
            state.effective_render_geometry(),
            Some(GeometryRef::Rectangle { .. })
        ));
        assert_eq!(state.effective_render_transform(), override_transform);
    }
}
