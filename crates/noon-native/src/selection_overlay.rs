//! Native presentation bookkeeping only. The session is the selection authority.

use super::{NativeApp, NativeHostError};
use noon::integration::PointerSelectionHighlight;
use noon_core::Color;
use noon_render_wgpu::AnalyticOverlay;

pub(super) fn prepare_highlight(
    highlight: Option<&PointerSelectionHighlight>,
) -> Result<Option<AnalyticOverlay>, NativeHostError> {
    highlight
        .map(|value| {
            AnalyticOverlay::new(
                &value.geometry,
                value.transform,
                Color::rgba(1.0, 1.0, 0.0, 0.35),
            )
        })
        .transpose()
        .map_err(|error| NativeHostError::Gpu(error.to_string()))
}

impl NativeApp {
    pub(super) fn selection_overlay_pending(&self) -> bool {
        let current = self.execution.session().pointer_selection_highlight();
        match (current.as_ref(), self.last_selection_highlight.as_ref()) {
            (None, None) => false,
            (Some(current), Some(presented)) => {
                current.target != presented.target
                    || current.geometry != presented.geometry
                    || current.transform != presented.transform
            }
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests;
