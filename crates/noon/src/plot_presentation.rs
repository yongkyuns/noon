//! Shared coordinate-label preparation and timestamp-aware sampled data.
mod preparation;
pub use preparation::{
    number_labels, NumberLabel, PlotPresentationError, TimeSeriesPlan, TimedPlotSample,
    MAX_PLOT_LABELS, MAX_TIMED_PLOT_SAMPLES,
};

#[cfg(any(feature = "native-text", feature = "latex"))]
mod label_options;
#[cfg(any(feature = "native-text", feature = "latex"))]
pub use label_options::{NumberLabelAuthoringError, NumberLabelOptions};
