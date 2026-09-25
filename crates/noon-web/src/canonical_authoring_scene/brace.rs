use super::{CanonicalAuthoringScene, PlayerOwnership};
use crate::authoring_error::AuthoringFailure;

impl CanonicalAuthoringScene {
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
                Err("live Brace construction requires an active canonical session".into())
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
            PlayerOwnership::Unstarted => {
                Err("live Brace mutation requires an active canonical session".into())
            }
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
            PlayerOwnership::Unstarted => {
                Err("live Brace mutation requires an active canonical session".into())
            }
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
}
