//! Coordinate adapters borrow the existing live facade; no second session.
use super::*;

impl SemanticExecutionPlayer {
    pub(crate) fn live_create_axes(
        &mut self,
        options: &noon::ManimAxesOptions,
    ) -> Result<noon::ManimAxes, AuthoringFailure> {
        // The outer result is session acquisition, the inner one preserves the
        // typed coordinate preparation/publication error until host projection.
        self.with_live_session(|live| Ok(live.axes(options)))?
            .map_err(crate::plot_error::coordinate_failure)
    }

    pub(crate) fn live_create_number_line(
        &mut self,
        options: &noon::ManimNumberLineOptions,
    ) -> Result<noon::ManimNumberLine, AuthoringFailure> {
        self.with_live_session(|live| Ok(live.number_line(options)))?
            .map_err(crate::plot_error::coordinate_failure)
    }
}
