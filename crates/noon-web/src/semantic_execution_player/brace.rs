use super::SemanticExecutionPlayer;
use crate::authoring_error::AuthoringFailure;

impl SemanticExecutionPlayer {
    pub(crate) fn live_brace_geometry_options(
        &mut self,
        target: &noon::LayoutAnchor,
        options: noon::BraceOptions,
    ) -> Result<noon::ManimGeometryOptions, AuthoringFailure> {
        self.source
            .live()
            .brace_geometry_options(target, options)
            .map_err(|error| AuthoringFailure::unclassified("brace.prepare", &error))
    }

    pub(crate) fn live_create_brace_label(
        &mut self,
        target: &noon::LayoutAnchor,
        label: noon::LayoutAnchor,
        options: noon::BraceOptions,
    ) -> Result<noon::BraceLabel, AuthoringFailure> {
        noon::BraceLabel::new_live(&mut self.source.live(), target, label, options)
            .map_err(|error| AuthoringFailure::unclassified("brace.create", &error))
    }

    pub(crate) fn live_shift_brace_label(
        &mut self,
        brace: &mut noon::BraceLabel,
        target: &noon::LayoutAnchor,
    ) -> Result<(), AuthoringFailure> {
        brace
            .shift_brace_live(&mut self.source.live(), target)
            .map_err(|error| AuthoringFailure::unclassified("brace.shift", &error))
    }

    pub(crate) fn live_change_brace_label(
        &mut self,
        brace: &mut noon::BraceLabel,
        target: &noon::LayoutAnchor,
        label: noon::LayoutAnchor,
    ) -> Result<(), AuthoringFailure> {
        brace
            .change_brace_label_live(&mut self.source.live(), target, label)
            .map_err(|error| AuthoringFailure::unclassified("brace.change", &error))
    }
    pub(crate) fn live_replace_brace_label(
        &mut self,
        brace: &mut noon::BraceLabel,
        label: noon::LayoutAnchor,
    ) -> Result<(), AuthoringFailure> {
        brace
            .change_label_live(&mut self.source.live(), label)
            .map_err(|error| AuthoringFailure::unclassified("brace.label", &error))
    }
}
