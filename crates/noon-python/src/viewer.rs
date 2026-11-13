//! Typed ownership transfer to the existing native window and capture hosts.
use crate::engine_error;
use pyo3::prelude::*;

#[pyclass(unsendable, module = "_noon_native")]
pub struct PreparedScene {
    execution: Option<noon::SceneExecution>,
    #[cfg(feature = "capture")]
    capture: Option<noon_export::SessionCapture>,
    #[cfg(feature = "capture")]
    dimensions: Option<(u32, u32)>,
}

impl PreparedScene {
    pub(crate) fn new(execution: noon::SceneExecution) -> Self {
        Self {
            execution: Some(execution),
            #[cfg(feature = "capture")]
            capture: None,
            #[cfg(feature = "capture")]
            dimensions: None,
        }
    }
}

impl PreparedScene {
    #[cfg(feature = "capture")]
    fn capture_pixels(&mut self, time: f64, width: u32, height: u32) -> PyResult<&[u8]> {
        let duration = self.duration()?;
        if !time.is_finite() || time < 0.0 {
            return Err(engine_error("snapshot time is outside the prepared scene"));
        }
        let execution = self
            .execution
            .as_mut()
            .ok_or_else(|| engine_error("native view is closed"))?;
        let interactive = execution.session().has_translation_drag_targets();
        if interactive {
            let roundoff = 8.0 * f64::EPSILON * duration.max(1.0);
            if (time - duration).abs() > roundoff {
                return Err(engine_error(
                    "translation drag snapshots require the authored endpoint time",
                ));
            }
        } else if time > duration {
            return Err(engine_error("snapshot time is outside the prepared scene"));
        }
        if self.dimensions != Some((width, height)) {
            self.capture = Some(
                noon_export::SessionCapture::new(
                    execution.session(),
                    noon_export::CaptureOptions::new(width, height),
                )
                .map_err(|e| engine_error(e.to_string()))?,
            );
            self.dimensions = Some((width, height));
        }
        if !interactive {
            execution
                .seek(time)
                .map_err(|e| engine_error(e.to_string()))?;
        }
        let frame = self
            .capture
            .as_mut()
            .expect("capture initialized")
            .capture(execution.session_mut())
            .map_err(|e| engine_error(e.to_string()))?;
        Ok(frame.rgba)
    }
}

#[pymethods]
impl PreparedScene {
    #[cfg(feature = "capture")]
    #[pyo3(signature = (*, time=0.0, width=960, height=540))]
    fn capture<'py>(
        &mut self,
        py: Python<'py>,
        time: f64,
        width: u32,
        height: u32,
    ) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
        self.capture_pixels(time, width, height)
            .map(|pixels| pyo3::types::PyBytes::new(py, pixels))
    }

    #[getter]
    fn duration(&self) -> PyResult<f64> {
        let session = self
            .execution
            .as_ref()
            .ok_or_else(|| engine_error("native view is closed"))?
            .session();
        if session.has_translation_drag_targets() {
            return Ok(session.frame().time);
        }
        Ok(session
            .replay_end_time()
            .or_else(|| session.finite_playback_end_time())
            .unwrap_or(0.0))
    }

    #[cfg(feature = "window")]
    #[pyo3(signature = (*, title="Noon", width=960, height=540))]
    fn show(&mut self, title: &str, width: u32, height: u32) -> PyResult<()> {
        if width == 0 || height == 0 {
            return Err(engine_error("window dimensions must be positive"));
        }
        let execution = self
            .execution
            .as_mut()
            .ok_or_else(|| engine_error("native view is closed"))?;
        let playback_controls = !execution.session().has_translation_drag_targets();
        if playback_controls {
            execution
                .seek(0.0)
                .map_err(|e| engine_error(e.to_string()))?;
        }
        let execution = self
            .execution
            .take()
            .ok_or_else(|| engine_error("native view is closed"))?;
        #[cfg(feature = "capture")]
        {
            self.capture = None;
        }
        noon_native::run_with_config(
            execution,
            noon_native::NativeViewportConfig {
                title: title.into(),
                width,
                height,
                playback_controls,
                inspection_zoom: true,
                ..Default::default()
            },
        )
        .map_err(|e| engine_error(e.to_string()))
    }

    /// An explicit output boundary: engine layers remain typed and in-process.
    #[cfg(feature = "capture")]
    #[pyo3(signature = (path, *, time=0.0, width=960, height=540))]
    fn snapshot(
        &mut self,
        path: std::path::PathBuf,
        time: f64,
        width: u32,
        height: u32,
    ) -> PyResult<()> {
        let pixels = self.capture_pixels(time, width, height)?;
        let file = std::fs::File::create_new(path).map_err(|e| engine_error(e.to_string()))?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| engine_error(e.to_string()))?;
        writer
            .write_image_data(pixels)
            .map_err(|e| engine_error(e.to_string()))?;
        writer.finish().map_err(|e| engine_error(e.to_string()))?;
        Ok(())
    }

    /// Consume this prepared view into a native H.264 MP4. Source runs only once.
    /// Rational frame rates use fps/fps_denominator; stop bounds are exclusive.
    /// Failures and cancellation preserve any previous destination. Reprepare
    /// the source for another export after this ownership transfer.
    #[cfg(feature = "capture")]
    #[pyo3(signature = (path, *, fps=30, fps_denominator=1, width=960, height=540,
        start_frame=0, frames=None, end_time=None, hold=0.0, max_frames=108_000,
        overwrite=false, ffmpeg="ffmpeg", crf=18, cancellation=None))]
    #[allow(clippy::too_many_arguments)]
    fn export_mp4(
        &mut self,
        py: Python<'_>,
        path: std::path::PathBuf,
        fps: u32,
        fps_denominator: u32,
        width: u32,
        height: u32,
        start_frame: u64,
        frames: Option<u64>,
        end_time: Option<f64>,
        hold: f64,
        max_frames: u64,
        overwrite: bool,
        ffmpeg: &str,
        crf: u8,
        cancellation: Option<&crate::export::ExportCancellation>,
    ) -> PyResult<crate::export::ExportSummary> {
        use noon::integration::{ExportFrameOptions, ExportStop, FrameRate};
        use pyo3::exceptions::PyValueError;

        if frames.is_some() && end_time.is_some() {
            return Err(PyValueError::new_err(
                "frames and end_time are mutually exclusive",
            ));
        }
        let frame_rate = FrameRate::new(fps, fps_denominator)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let frame_options = ExportFrameOptions {
            frame_rate,
            start_frame,
            stop: match (frames, end_time) {
                (Some(count), _) => ExportStop::FrameCount(count),
                (_, Some(end)) => ExportStop::EndTime(end),
                _ => ExportStop::SourceEnd,
            },
            max_frames,
            max_transitions_per_sample: 4_096,
            final_hold_seconds: hold,
        };
        let mut capture = noon_export::CaptureOptions::new(width, height);
        if let Some(cancellation) = cancellation {
            capture.cancellation = cancellation.token.clone();
        }
        let token = capture.cancellation.clone();
        let mut output = noon_export::output::OutputOptions::mp4(path);
        output.overwrite = overwrite;
        output.ffmpeg = ffmpeg.into();
        output.crf = crf;

        if self
            .execution
            .as_ref()
            .is_some_and(|execution| execution.session().has_translation_drag_targets())
        {
            return Err(engine_error(
                "translation drag scenes cannot be exported as finite MP4",
            ));
        }
        let mut execution = self
            .execution
            .take()
            .ok_or_else(|| engine_error("native view is closed"))?;
        self.capture = None;
        self.dimensions = None;
        let mut signal_error = None;
        let result = noon_export::output::export_scene_file_with_interrupt(
            &mut execution,
            frame_options,
            capture,
            output,
            || {
                // Only the GIL is released; the non-Send scene remains on this
                // owner's stack. Other Python threads may set the atomic token.
                py.detach(std::thread::yield_now);
                if let Err(error) = py.check_signals() {
                    token.cancel();
                    signal_error = Some(error);
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "Python export interrupted",
                    ));
                }
                Ok(())
            },
        );
        let retired = execution.retire_execution_source();
        if let Some(error) = signal_error {
            return Err(error);
        }
        let summary = result.map_err(|error| engine_error(error.to_string()))?;
        retired.map_err(|error| engine_error(error.to_string()))?;
        Ok(summary.into())
    }

    fn close(&mut self) -> PyResult<()> {
        if let Some(execution) = self.execution.as_mut() {
            execution
                .retire_execution_source()
                .map_err(|e| engine_error(e.to_string()))?;
        }
        #[cfg(feature = "capture")]
        {
            self.capture = None;
        }
        self.execution = None;
        Ok(())
    }
}
