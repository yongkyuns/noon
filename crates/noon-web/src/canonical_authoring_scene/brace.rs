use super::CanonicalAuthoringScene;
use crate::authoring_error::AuthoringFailure;

impl CanonicalAuthoringScene {
    pub(crate) fn live_brace_geometry_options(
        &mut self,
        target: &noon::LayoutAnchor,
        options: noon::BraceOptions,
    ) -> Result<noon::ManimGeometryOptions, AuthoringFailure> {
        self.local_authoring_scene()?
            .brace_geometry_options(target, options)
            .map_err(|error| AuthoringFailure::unclassified("brace.prepare", &error))
    }

    pub(crate) fn live_create_brace_label(
        &mut self,
        target: &noon::LayoutAnchor,
        label: noon::LayoutAnchor,
        options: noon::BraceOptions,
    ) -> Result<noon::BraceLabel, AuthoringFailure> {
        noon::BraceLabel::new(self.local_authoring_scene()?, target, label, options)
            .map_err(|error| AuthoringFailure::unclassified("brace.create", &error))
    }

    pub(crate) fn live_shift_brace_label(
        &mut self,
        brace: &mut noon::BraceLabel,
        target: &noon::LayoutAnchor,
    ) -> Result<(), AuthoringFailure> {
        brace
            .shift_brace(self.local_authoring_scene()?, target)
            .map_err(|error| AuthoringFailure::unclassified("brace.shift", &error))
    }

    pub(crate) fn live_change_brace_label(
        &mut self,
        brace: &mut noon::BraceLabel,
        target: &noon::LayoutAnchor,
        label: noon::LayoutAnchor,
    ) -> Result<(), AuthoringFailure> {
        let code = if self.player_ownership.is_unstarted() {
            "brace.replace"
        } else {
            "brace.change"
        };
        brace
            .change_brace_label(self.local_authoring_scene()?, target, label)
            .map_err(|error| AuthoringFailure::unclassified(code, &error))
    }

    pub(crate) fn live_replace_brace_label(
        &mut self,
        brace: &mut noon::BraceLabel,
        label: noon::LayoutAnchor,
    ) -> Result<(), AuthoringFailure> {
        brace
            .change_label(self.local_authoring_scene()?, label)
            .map_err(|error| AuthoringFailure::unclassified("brace.label", &error))
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
