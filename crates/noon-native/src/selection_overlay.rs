//! Native presentation bookkeeping only. The session is the selection authority.

use super::{NativeApp, NativeHostError};
use noon::integration::PointerSelectionPresentation;
use noon_render_wgpu::AnalyticOverlay;

pub(super) fn prepare_highlight(
    highlight: Option<&PointerSelectionPresentation>,
) -> Result<Option<AnalyticOverlay>, NativeHostError> {
    highlight
        .map(|value| AnalyticOverlay::new(&value.geometry, value.transform, value.color))
        .transpose()
        .map_err(|error| NativeHostError::Gpu(error.to_string()))
}

impl NativeApp {
    pub(super) fn selection_overlay_pending(&self) -> bool {
        self.execution.session().pointer_selection_presentation()
            != self.last_selection_presentation
    }
}

#[cfg(test)]
mod tests;
