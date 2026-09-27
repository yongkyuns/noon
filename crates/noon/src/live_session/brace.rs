//! Explicit live-session Brace queries use the common coherent observation policy.
use super::*;

impl LiveSession<'_> {
    /// Prepare Brace geometry from the current effective target without publication.
    pub fn brace_geometry_options(
        &self,
        target: &crate::LayoutAnchor,
        options: crate::BraceOptions,
    ) -> Result<crate::ManimGeometryOptions, LiveSessionError> {
        crate::brace_authoring::layout::prepare_geometry(
            self.store,
            Some(self.session),
            target,
            None,
            options,
        )
        .map(|prepared| prepared.options)
        .map_err(Into::into)
    }

    pub(crate) fn prepare_brace_geometry(
        &self,
        target: &crate::LayoutAnchor,
        label: &crate::LayoutAnchor,
        options: crate::BraceOptions,
    ) -> Result<crate::geometry_authoring::PreparedBraceGeometry, LiveSessionError> {
        crate::brace_authoring::layout::prepare_geometry(
            self.store,
            Some(self.session),
            target,
            Some(label),
            options,
        )
        .map_err(Into::into)
    }

    pub(crate) fn require_brace_label_placement(
        &self,
        label: &crate::LayoutAnchor,
    ) -> Result<(), LiveSessionError> {
        crate::brace_authoring::layout::require_label_placement(
            self.store,
            Some(self.session),
            label,
        )
        .map_err(Into::into)
    }
}
