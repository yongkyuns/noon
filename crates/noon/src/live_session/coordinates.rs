//! Coordinate construction through the ordinary running publication transaction.
use super::*;
use crate::coordinate_authoring::{
    prepare_axes, prepare_number_line, prepare_number_plane, resolve_family,
};
use crate::AuthoringError;
use crate::{
    AxesFrame, CoordinateAuthoringError, ManimAxes, ManimAxesOptions, ManimNumberLine,
    ManimNumberLineOptions, ManimNumberPlane, ManimNumberPlaneOptions, NumberLineFrame,
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

    /// Construct a detached retained NumberPlane through this execution's
    /// ordinary publication transaction.
    pub fn number_plane(
        &mut self,
        options: &ManimNumberPlaneOptions,
    ) -> Result<ManimNumberPlane, CoordinateAuthoringError> {
        let (transaction, root) = prepare_number_plane(options)?;
        let result = self.apply(transaction)?;
        ManimNumberPlane::from_family(resolve_family(Rc::clone(self.store), &result, root)?)
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
        axes.snapshot_with(&mut |shaft| self.coordinate_path_query(shaft))
    }

    pub fn effective_number_plane_frame(
        &self,
        plane: &ManimNumberPlane,
    ) -> Result<AxesFrame, CoordinateAuthoringError> {
        self.require_family(plane.family())?;
        self.require_target_capture()?;
        Ok(AxesFrame::new(
            plane
                .x_axis()?
                .snapshot_with(&mut |shaft| self.coordinate_path_query(shaft))?,
            plane
                .y_axis()?
                .snapshot_with(&mut |shaft| self.coordinate_path_query(shaft))?,
        ))
    }

    pub fn effective_number_line_frame(
        &self,
        line: &ManimNumberLine,
    ) -> Result<NumberLineFrame, CoordinateAuthoringError> {
        self.require_family(line.family())?;
        self.require_target_capture()?;
        line.snapshot_with(&mut |shaft| self.coordinate_path_query(shaft))
    }

    fn coordinate_path_query(&self, shaft: &Mobject) -> Result<crate::PathQuery, AuthoringError> {
        if self.session.semantic_object_is_reachable(shaft.node_id()) {
            crate::path_queries::effective_path_query(self.store, self.session, shaft)
        } else {
            // Preserve the ordinary detached-capture restrictions, including
            // uncompiled reactive bindings. Do not select this branch on error.
            crate::effective_capture::capture_mobject_state(self.store, self.session, shaft)?;
            shaft.path_query()
        }
    }
}

#[cfg(test)]
mod tests;
