//! Coordinate construction through the ordinary running publication transaction.
use super::*;
use crate::coordinate_authoring::{prepare_axes, prepare_number_line, resolve_family};
use crate::{
    AxesFrame, CoordinateAuthoringError, ManimAxes, ManimAxesOptions, ManimNumberLine,
    ManimNumberLineOptions, NumberLineFrame,
};

impl LiveSession<'_> {
    /// Prepare and publish all shafts, ticks and families atomically. The result
    /// is detached: construction alone neither admits nor renders the axes.
    pub fn axes(
        &mut self,
        options: &ManimAxesOptions,
    ) -> Result<ManimAxes, CoordinateAuthoringError> {
        let (transaction, root) = prepare_axes(options)?;
        let result = self.apply(transaction)?;
        ManimAxes::from_family(resolve_family(Rc::clone(self.store), &result, root)?)
    }

    /// Construct a detached NumberLine through this existing execution owner.
    /// Invalid input and stale/foreign publication failures allocate no identity.
    pub fn number_line(
        &mut self,
        options: &ManimNumberLineOptions,
    ) -> Result<ManimNumberLine, CoordinateAuthoringError> {
        let (transaction, root) = prepare_number_line(options)?;
        let result = self.apply(transaction)?;
        ManimNumberLine::from_family(resolve_family(Rc::clone(self.store), &result, root)?)
    }

    /// Observe both shafts at this owner's current coherent publication. Shared
    /// path-query semantics also support its detached targets without admission.
    pub fn effective_axes_frame(
        &self,
        axes: &ManimAxes,
    ) -> Result<AxesFrame, CoordinateAuthoringError> {
        axes.snapshot_with(&mut |shaft| {
            crate::path_queries::effective_path_query(self.store, self.session, shaft)
        })
    }

    pub fn effective_number_line_frame(
        &self,
        line: &ManimNumberLine,
    ) -> Result<NumberLineFrame, CoordinateAuthoringError> {
        line.snapshot_with(&mut |shaft| {
            crate::path_queries::effective_path_query(self.store, self.session, shaft)
        })
    }
}

#[cfg(test)]
mod tests;
