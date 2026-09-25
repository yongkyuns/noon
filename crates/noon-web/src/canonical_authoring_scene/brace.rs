use super::{CanonicalAuthoringScene, PlayerOwnership};
use crate::authoring_error::AuthoringFailure;

impl CanonicalAuthoringScene {
    pub(crate) fn live_brace_geometry_options(
        &mut self,
        target: &noon::LayoutAnchor,
        options: noon::BraceOptions,
    ) -> Result<noon::ManimGeometryOptions, AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_brace_geometry_options(target, options),
            PlayerOwnership::Unstarted => self
                .scene
                .brace_geometry_options(target, options)
                .map_err(|error| AuthoringFailure::unclassified("brace.prepare", &error)),
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    pub(crate) fn live_create_brace_label(
        &mut self,
        target: &noon::LayoutAnchor,
        label: noon::LayoutAnchor,
        options: noon::BraceOptions,
    ) -> Result<noon::BraceLabel, AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_create_brace_label(target, label, options),
            PlayerOwnership::Unstarted => {
                noon::BraceLabel::new(&mut self.scene, target, label, options)
                    .map_err(|error| AuthoringFailure::unclassified("brace.create", &error))
            }
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }
    pub(crate) fn live_shift_brace_label(
        &mut self,
        brace: &mut noon::BraceLabel,
        target: &noon::LayoutAnchor,
    ) -> Result<(), AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_shift_brace_label(brace, target),
            PlayerOwnership::Unstarted => brace
                .shift_brace(&mut self.scene, target)
                .map_err(|error| AuthoringFailure::unclassified("brace.shift", &error)),
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }
    pub(crate) fn live_change_brace_label(
        &mut self,
        brace: &mut noon::BraceLabel,
        target: &noon::LayoutAnchor,
        label: noon::LayoutAnchor,
    ) -> Result<(), AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_change_brace_label(brace, target, label),
            PlayerOwnership::Unstarted => brace
                .change_brace_label(&mut self.scene, target, label)
                .map_err(|error| AuthoringFailure::unclassified("brace.replace", &error)),
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }
    pub(crate) fn live_replace_brace_label(
        &mut self,
        brace: &mut noon::BraceLabel,
        label: noon::LayoutAnchor,
    ) -> Result<(), AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_replace_brace_label(brace, label),
            PlayerOwnership::Unstarted => brace
                .change_label(&mut self.scene, label)
                .map_err(|error| AuthoringFailure::unclassified("brace.label", &error)),
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }
}

impl super::wasm::CanonicalAuthoringSceneContext {
    pub(crate) fn create_live_brace_label(
        &mut self,
        target: &noon::LayoutAnchor,
        label: noon::LayoutAnchor,
        options: noon::BraceOptions,
    ) -> Result<noon::BraceLabel, AuthoringFailure> {
        self.inner.live_create_brace_label(target, label, options)
    }
    pub(crate) fn shift_live_brace_label(
        &mut self,
        brace: &mut noon::BraceLabel,
        target: &noon::LayoutAnchor,
    ) -> Result<(), AuthoringFailure> {
        self.inner.live_shift_brace_label(brace, target)
    }
    pub(crate) fn change_live_brace_label(
        &mut self,
        brace: &mut noon::BraceLabel,
        target: &noon::LayoutAnchor,
        label: noon::LayoutAnchor,
    ) -> Result<(), AuthoringFailure> {
        self.inner.live_change_brace_label(brace, target, label)
    }
    pub(crate) fn replace_live_brace_label(
        &mut self,
        brace: &mut noon::BraceLabel,
        label: noon::LayoutAnchor,
    ) -> Result<(), AuthoringFailure> {
        self.inner.live_replace_brace_label(brace, label)
    }
}
