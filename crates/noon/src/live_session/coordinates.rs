//! Coordinate construction through the ordinary running publication transaction.
use super::*;
use crate::coordinate_authoring::{
    coordinate_path_query, prepare_axes, prepare_number_line, publish_prepared_axes,
    publish_prepared_number_line,
};
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
        let prepared = prepare_axes(options)?;
        let root = self.with_semantic_publication(|store, publish| {
            publish_prepared_axes(options, prepared.axes, prepared.tip_paths, store, publish)
        })?;
        ManimAxes::from_family(MobjectFamily::from_node(Rc::clone(self.store), root)?)
    }

    /// Construct a detached NumberLine through this existing execution owner.
    /// Invalid input and stale/foreign publication failures allocate no identity.
    pub fn number_line(
        &mut self,
        options: &ManimNumberLineOptions,
    ) -> Result<ManimNumberLine, CoordinateAuthoringError> {
        let prepared = prepare_number_line(options)?;
        let root = self.with_semantic_publication(|store, publish| {
            publish_prepared_number_line(prepared, store, publish)
        })?;
        ManimNumberLine::from_family(MobjectFamily::from_node(Rc::clone(self.store), root)?)
    }

    /// Observe both shafts through one coherent publication. Reachable shafts
    /// use effective path state; detached shafts have no execution row and use
    /// their validated authored state, as with ordinary detached target capture.
    /// A stale/foreign session or pending callback never falls back to authored.
    pub fn effective_axes_frame(
        &self,
        axes: &ManimAxes,
    ) -> Result<AxesFrame, CoordinateAuthoringError> {
        self.require_family(axes.family())?;
        self.require_target_capture()?;
        axes.snapshot_with(&mut |shaft| coordinate_path_query(self.store, self.session, shaft))
    }

    pub fn effective_number_line_frame(
        &self,
        line: &ManimNumberLine,
    ) -> Result<NumberLineFrame, CoordinateAuthoringError> {
        self.require_family(line.family())?;
        self.require_target_capture()?;
        line.snapshot_with(&mut |shaft| coordinate_path_query(self.store, self.session, shaft))
    }
}

#[cfg(test)]
mod tests;
