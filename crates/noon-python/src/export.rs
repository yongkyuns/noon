//! Python values at the native file-output boundary; Rust owns frame sampling.

use pyo3::prelude::*;

/// Cooperative cancellation shared with the native capture and encoder owners.
#[pyclass(frozen, module = "_noon_native")]
#[derive(Default)]
pub struct ExportCancellation {
    pub(crate) token: noon_export::CaptureCancellation,
}

#[pymethods]
impl ExportCancellation {
    #[new]
    fn new() -> Self {
        Self::default()
    }

    fn cancel(&self) {
        self.token.cancel();
    }

    #[getter]
    fn cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
}

/// A completed native output transaction, returned only after publication.
#[pyclass(frozen, get_all, module = "_noon_native")]
pub struct ExportSummary {
    pub(crate) path: std::path::PathBuf,
    pub(crate) frames: u64,
    pub(crate) fps: u32,
    pub(crate) fps_denominator: u32,
    pub(crate) start_time: f64,
    pub(crate) end_time: f64,
    pub(crate) source_end: Option<f64>,
    pub(crate) reason: &'static str,
}

impl From<noon_export::output::FileExportSummary> for ExportSummary {
    fn from(value: noon_export::output::FileExportSummary) -> Self {
        let sampling = value.capture.sampling;
        Self {
            path: value.path,
            frames: sampling.frames,
            fps: sampling.frame_rate.numerator(),
            fps_denominator: sampling.frame_rate.denominator(),
            start_time: sampling.start_time,
            end_time: sampling.end_time,
            source_end: sampling.source_end,
            reason: match sampling.reason {
                noon::integration::ExportEndReason::SourceEnd => "source_end",
                noon::integration::ExportEndReason::RequestedStop => "requested_stop",
            },
        }
    }
}
