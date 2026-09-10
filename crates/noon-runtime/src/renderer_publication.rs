use std::{collections::BTreeSet, sync::Arc};

use noon_core::{
    FontResourceLookup, GeometryRef, GeometryResourceLookup, ObjectContentRef, PublicationContext,
    Rect, RetainedFamilyAnimationPlan, Style, TextResourceHandle, TextResourceLookup, Transform2D,
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
    /// effective stable frame they were derived from. Existing runtime callers keep
    /// the empty default, so this is additive until a transform-family owner supplies
    /// the transient rows explicitly.
    pub fn with_derived_display_objects(
        mut self,
        derived_display_objects: &'a [DerivedDisplayObject],
    ) -> Self {
        self.derived_display_objects = derived_display_objects;
        self
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

#[cfg(test)]
mod derived_display_tests {
    use noon_core::{GeometryRef, Style, Vec2};

    use super::*;

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

    #[test]
    fn derived_display_row_reuses_retained_content_without_execution_identity() {
        let state = derived_state();

        assert!(matches!(
            state.effective_render_geometry(),
            Some(GeometryRef::Circle { radius }) if *radius == 1.0
        ));
        assert_eq!(state.effective_render_transform(), state.transform);
        // The type itself has no semantic or execution identity field: the only
        // retained object reference is immutable content/resource state.
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
