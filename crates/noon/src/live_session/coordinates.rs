//! Coordinate construction through the ordinary running publication transaction.
use super::*;
use crate::coordinate_authoring::{
    prepare_axes, prepare_number_line, prepare_number_plane, prepare_polar_plane, resolve_family,
};
use crate::AuthoringError;
use crate::{
    AxesFrame, CoordinateAuthoringError, ManimAxes, ManimAxesOptions, ManimBarChart,
    ManimBarChartOptions, ManimNumberLine, ManimNumberLineOptions, ManimNumberPlane,
    ManimNumberPlaneOptions, ManimPolarPlane, ManimPolarPlaneOptions, NumberLineFrame, PolarFrame,
};

impl LiveSession<'_> {
    /// Construct the chart's axes, retained bars and nested families in one
    /// running publication transaction.
    pub fn bar_chart(
        &mut self,
        options: &ManimBarChartOptions,
    ) -> Result<ManimBarChart, CoordinateAuthoringError> {
        let (transaction, chart, axes, bars) =
            crate::coordinate_authoring::bar_chart::prepare(options)?;
        let result = self.apply(transaction)?;
        ManimBarChart::from_result(Rc::clone(self.store), &result, chart, axes, bars)
    }
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

    /// Construct a detached retained PolarPlane through this execution's
    /// ordinary publication transaction.
    pub fn polar_plane(
        &mut self,
        options: &ManimPolarPlaneOptions,
    ) -> Result<ManimPolarPlane, CoordinateAuthoringError> {
        let (transaction, root) = prepare_polar_plane(options)?;
        let result = self.apply(transaction)?;
        ManimPolarPlane::from_family(resolve_family(Rc::clone(self.store), &result, root)?)
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

    pub fn effective_polar_plane_frame(
        &self,
        plane: &ManimPolarPlane,
    ) -> Result<PolarFrame, CoordinateAuthoringError> {
        self.require_family(plane.family())?;
        self.require_target_capture()?;
        Ok(PolarFrame::new(AxesFrame::new(
            plane
                .x_axis()?
                .snapshot_with(&mut |shaft| self.coordinate_path_query(shaft))?,
            plane
                .y_axis()?
                .snapshot_with(&mut |shaft| self.coordinate_path_query(shaft))?,
        )))
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

#[cfg(all(feature = "native-text", feature = "latex"))]
impl LiveSession<'_> {
    /// Create the complete labeled chart through the current execution owner.
    pub fn bar_chart_with_axis_labels(
        &mut self,
        options: &ManimBarChartOptions,
        labels: &crate::plot_presentation::NumberLabelOptions,
        backend: &mut impl crate::LatexBackend,
    ) -> Result<ManimBarChart, crate::plot_presentation::NumberLabelAuthoringError> {
        let prepared = crate::coordinate_authoring::bar_chart::PreparedLabeledChart::prepare(
            options, labels, backend,
        )?;
        let store = Rc::clone(self.store);
        let (result, [chart, axes, bars]) =
            prepared.publish(&mut store.borrow_mut(), |store, transaction| {
                self.session
                    .apply_semantic_transaction_at_root(store, self.root, transaction)
                    .map_err(AuthoringError::from)
                    .map_err(crate::TextAuthoringError::Semantic)
            })?;
        ManimBarChart::from_result(store, &result, chart, axes, bars).map_err(Into::into)
    }

    /// Create a retained family of value labels from one coherent live snapshot.
    pub fn bar_labels(
        &mut self,
        chart: &ManimBarChart,
        backend: &mut impl crate::LatexBackend,
        options: &crate::BarLabelOptions,
    ) -> Result<crate::MobjectFamily, crate::plot_presentation::NumberLabelAuthoringError> {
        self.require_family(chart.family())
            .map_err(CoordinateAuthoringError::from)?;
        let prepared = crate::coordinate_authoring::bar_chart::labels::PreparedBarLabels::prepare(
            chart,
            backend,
            options,
            |object| self.capture_mobject_state(object).map_err(Into::into),
        )?;
        let store = Rc::clone(self.store);
        let (_, node) = prepared.publish_into(
            &mut store.borrow_mut(),
            None,
            noon_core::SemanticMutationTransaction::new(),
            |store, transaction| {
                self.session
                    .apply_semantic_transaction_at_root(store, self.root, transaction)
                    .map_err(AuthoringError::from)
                    .map_err(crate::TextAuthoringError::Semantic)
            },
        )?;
        crate::MobjectFamily::from_node(store, node).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests;
