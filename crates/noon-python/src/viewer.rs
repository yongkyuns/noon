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
        if !time.is_finite() || time < 0.0 || time > duration {
            return Err(engine_error("snapshot time is outside the prepared scene"));
        }
        let execution = self
            .execution
            .as_mut()
            .ok_or_else(|| engine_error("native view is closed"))?;
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
        execution
            .seek(time)
            .map_err(|e| engine_error(e.to_string()))?;
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
        self.execution
            .as_mut()
            .ok_or_else(|| engine_error("native view is closed"))?
            .seek(0.0)
            .map_err(|e| engine_error(e.to_string()))?;
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
                playback_controls: true,
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
