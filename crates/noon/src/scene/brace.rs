//! Scene-owned Brace queries share the explicit live-session observation policy.
use super::*;

impl Scene {
    pub(crate) fn prepare_brace_geometry(
        &self,
        target: &crate::LayoutAnchor,
        label: Option<&crate::LayoutAnchor>,
        options: crate::BraceOptions,
    ) -> Result<crate::geometry_authoring::PreparedBraceGeometry, AuthoringError> {
        crate::brace_authoring::layout::prepare_geometry(
            &self.store,
            self.execution.as_ref(),
            target,
            label,
            options,
        )
    }

    pub(crate) fn require_brace_label_placement(
        &self,
        label: &crate::LayoutAnchor,
    ) -> Result<(), AuthoringError> {
        crate::brace_authoring::layout::require_label_placement(
            &self.store,
            self.execution.as_ref(),
            label,
        )
    }
}
