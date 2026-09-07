use std::collections::BTreeSet;

use noon_core::{
    FontResourceLookup, GeometryResourceLookup, PublicationContext, RetainedFamilyAnimationPlan,
    TextResourceLookup,
};

use crate::{FrameChanges, FrameState, RetainedPlannedFamilyFrame};

/// One coherent borrowed runtime publication for renderer preparation.
///
/// The runtime creates this only while consuming accumulated changes. It keeps an
/// effective frame, its immutable projected resources, and their typed publication
/// context together without copying a second render world.
pub struct RendererPublication<'a> {
    context: PublicationContext,
    frame: &'a FrameState,
    changes: FrameChanges,
    text_resources: &'a dyn TextResourceLookup,
    font_resources: &'a dyn FontResourceLookup,
    geometry_resources: &'a dyn GeometryResourceLookup,
    family_animation_plans: &'a [RetainedFamilyAnimationPlan],
    active_family_animation_indices: &'a BTreeSet<usize>,
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

    /// Escalate an acquired redraw to a full renderer invalidation while retaining
    /// this publication's exact frame, resources, and revision context.
    pub fn invalidate_all(&mut self) {
        self.changes = FrameChanges::all();
    }
}

impl<'a> RendererPublication<'a> {
    pub(crate) fn new(
        context: PublicationContext,
        frame: &'a FrameState,
        changes: FrameChanges,
        text_resources: &'a dyn TextResourceLookup,
        font_resources: &'a dyn FontResourceLookup,
        geometry_resources: &'a dyn GeometryResourceLookup,
        family_animation_plans: &'a [RetainedFamilyAnimationPlan],
        active_family_animation_indices: &'a BTreeSet<usize>,
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
        }
    }
}
