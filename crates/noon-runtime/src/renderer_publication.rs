use std::{collections::BTreeSet, sync::Arc};

use noon_core::{
    Color, FontResourceLookup, GeometryRef, GeometryResourceLookup, ObjectContentRef,
    PublicationContext, Rect, RetainedFamilyAnimationPlan, Style, TextResourceHandle,
    TextResourceLookup, Transform2D,
};

use crate::{FrameChanges, FrameState, RetainedPlannedFamilyFrame};

/// One identity-free effective visual row derived for renderer-only animation work.
///
/// Unlike [`crate::FrameObjectState`], this type deliberately has no `ObjectId` and
/// therefore cannot enter the stable execution-slot domain. It is suitable for
/// temporary family-Transform padding copies whose lifetime is bounded by one
/// animation plan/publication. Content still uses the same immutable retained
/// resources as ordinary frame rows; there is no second geometry/text world.
#[derive(Clone, Debug, PartialEq)]
pub struct DerivedDisplayObjectState {
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

/// One transient visual occurrence and its placement provenance.
///
/// `anchor_object_index` identifies an existing stable execution slot only for
/// painter placement and source-local invalidation. It is not the identity of this
/// occurrence. `occurrence_index` preserves deterministic order when multiple
/// derived copies share one source anchor.
#[derive(Clone, Debug, PartialEq)]
pub struct DerivedDisplayObject {
    anchor_object_index: u32,
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
            occurrence_index,
            state,
        }
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
                "derived display anchor {anchor_object_index} is outside stable frame object count {object_count}"
            ),
            Self::AnchorNotPresent(index) => {
                write!(formatter, "derived display anchor {index} is not present in this frame")
            }
            Self::DuplicateOccurrence(index) => {
                write!(formatter, "derived display occurrence index {index} is duplicated")
            }
            Self::InvalidZIndex(index) => {
                write!(formatter, "derived display occurrence {index} has invalid z-index")
            }
            Self::InvalidTransform(index) => {
                write!(formatter, "derived display occurrence {index} has invalid transform")
            }
            Self::InvalidStyle(index) => {
                write!(formatter, "derived display occurrence {index} has invalid style")
            }
            Self::InvalidAppearance(index) => {
                write!(formatter, "derived display occurrence {index} has invalid appearance")
            }
            Self::InvalidReveal(index) => {
                write!(formatter, "derived display occurrence {index} has invalid reveal")
            }
            Self::InvalidMorph(index) => {
                write!(formatter, "derived display occurrence {index} has invalid morph")
            }
        }
    }
}

impl std::error::Error for DerivedDisplayPublicationError {}

/// One coherent borrowed runtime publication for renderer preparation.
///
/// The runtime creates this only while consuming accumulated changes. It keeps an
/// effective frame, its immutable projected resources, and their typed publication
/// context together without copying a second render world. Optional derived display
/// rows are borrowed animation output layered on that publication; they never enter
/// `FrameState` or stable execution storage.
pub struct RendererPublication<'a> {
    context: PublicationContext,
    frame: &'a FrameState,
    changes: FrameChanges,
    text_resources: &'a dyn TextResourceLookup,
    font_resources: &'a dyn FontResourceLookup,
    geometry_resources: &'a dyn GeometryResourceLookup,
    family_animation_plans: &'a [RetainedFamilyAnimationPlan],
    active_family_animation_indices: &'a BTreeSet<usize>,
    painter_order: &'a [u32],
    derived_display_objects: &'a [DerivedDisplayObject],
}

impl RendererPublication<'_> {
    pub const fn context(&self) -> PublicationContext {
        self.context
    }

    pub const fn frame(&self) -> &FrameState {
        self.frame
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

    pub fn geometry_resources(&self) -> &dyn GeometryResourceLookup {
        self.geometry_resources
    }

    pub fn planned_family_frame(&self) -> RetainedPlannedFamilyFrame<'_> {
        RetainedPlannedFamilyFrame {
            retained: self.frame,
            family_animations: &self.frame.family_animations,
            family_plan_indices: &self.frame.family_animation_plan_indices,
        }
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
    /// Attach animation-derived display rows to the exact publication that owns the
    /// effective stable frame they were derived from.
    ///
    /// Every anchor and value is checked before the borrowed slice is installed, so
    /// failure leaves the publication unchanged. Existing runtime callers keep the
    /// empty default until a transform-family owner supplies transient rows.
    pub fn with_derived_display_objects(
        mut self,
        derived_display_objects: &'a [DerivedDisplayObject],
    ) -> Result<Self, DerivedDisplayPublicationError> {
        validate_derived_display_objects(self.frame, derived_display_objects)?;
        self.derived_display_objects = derived_display_objects;
        Ok(self)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        context: PublicationContext,
        frame: &'a FrameState,
        changes: FrameChanges,
        text_resources: &'a dyn TextResourceLookup,
        font_resources: &'a dyn FontResourceLookup,
        geometry_resources: &'a dyn GeometryResourceLookup,
        family_animation_plans: &'a [RetainedFamilyAnimationPlan],
        active_family_animation_indices: &'a BTreeSet<usize>,
        painter_order: &'a [u32],
    ) -> Self {
        Self {
            context,
            frame,
            changes,
            text_resources,
            font_resources,
            geometry_resources,
            family_animation_plans,
            active_family_animation_indices,
            painter_order,
            derived_display_objects: &[],
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
                id: ObjectId::new(1),
                z_index: 0.0,
                content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                text_bounds: None,
                transform: Transform2D::IDENTITY,
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
