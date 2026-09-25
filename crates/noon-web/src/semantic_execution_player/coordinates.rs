//! Coordinate adapters borrow the existing live facade; no second session.
use super::*;

impl SemanticExecutionPlayer {
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_create_path_family(
        &mut self,
        paths: Vec<(noon::VectorPath, noon_core::SemanticStyle)>,
    ) -> Result<noon::MobjectFamily, AuthoringFailure> {
        self.with_live_session(|live| Ok(live.create_path_family(paths)))?
            .map_err(AuthoringFailure::from)
    }

    pub(crate) fn live_create_polar_plane(
        &mut self,
        options: &noon::ManimPolarPlaneOptions,
    ) -> Result<noon::ManimPolarPlane, AuthoringFailure> {
        self.with_live_session(|live| Ok(live.polar_plane(options)))?
            .map_err(crate::plot_error::coordinate_failure)
    }

    pub(crate) fn live_create_axes(
        &mut self,
        options: &noon::ManimAxesOptions,
    ) -> Result<noon::ManimAxes, AuthoringFailure> {
        // The outer result is session acquisition, the inner one preserves the
        // typed coordinate preparation/publication error until host projection.
        self.with_live_session(|live| Ok(live.axes(options)))?
            .map_err(crate::plot_error::coordinate_failure)
    }

    pub(crate) fn live_create_bar_chart(
        &mut self,
        options: &noon::ManimBarChartOptions,
    ) -> Result<noon::ManimBarChart, AuthoringFailure> {
        self.with_live_session(|live| Ok(live.bar_chart(options)))?
            .map_err(crate::plot_error::coordinate_failure)
    }

    pub(crate) fn live_change_bar_values(
        &mut self,
        chart: &mut noon::ManimBarChart,
        values: &[f64],
        update_colors: bool,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| {
            Ok(chart.change_bar_values_live(live, values, update_colors))
        })?
        .map_err(crate::plot_error::coordinate_failure)
    }

    pub(crate) fn live_create_labeled_bar_chart(
        &mut self,
        options: &noon::ManimBarChartOptions,
        labels: &noon::plot_presentation::NumberLabelOptions,
        compiler: &mut crate::WasmLatexCompiler,
    ) -> Result<noon::ManimBarChart, AuthoringFailure> {
        self.with_live_session(|live| {
            Ok(live.bar_chart_with_axis_labels(options, labels, compiler))
        })?
        .map_err(|error| {
            AuthoringFailure::new("invalid_input", "plot.bar_chart", error.to_string())
        })
    }

    pub(crate) fn live_bar_labels(
        &mut self,
        chart: &noon::ManimBarChart,
        compiler: &mut crate::WasmLatexCompiler,
        options: &noon::BarLabelOptions,
    ) -> Result<noon::MobjectFamily, AuthoringFailure> {
        self.with_live_session(|live| Ok(live.bar_labels(chart, compiler, options)))?
            .map_err(|error| {
                AuthoringFailure::new("invalid_input", "plot.bar_labels", error.to_string())
            })
    }

    pub(crate) fn live_create_number_line(
        &mut self,
        options: &noon::ManimNumberLineOptions,
    ) -> Result<noon::ManimNumberLine, AuthoringFailure> {
        self.with_live_session(|live| Ok(live.number_line(options)))?
            .map_err(crate::plot_error::coordinate_failure)
    }

    pub(crate) fn live_create_number_plane(
        &mut self,
        options: &noon::ManimNumberPlaneOptions,
    ) -> Result<noon::ManimNumberPlane, AuthoringFailure> {
        self.with_live_session(|live| Ok(live.number_plane(options)))?
            .map_err(crate::plot_error::coordinate_failure)
    }
}
