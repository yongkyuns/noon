//! Native bindings for Scene-owned PCM resources and clip identities.
use crate::engine_error;
use noon::integration::SemanticStore;
use noon::{
    AudioClipDefinition, AudioClipOptions, AudioResource, DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
};
use noon_core::SemanticNodeId;
use pyo3::{buffer::PyBuffer, prelude::*};
use std::{cell::RefCell, rc::Rc};

#[pyclass(skip_from_py_object, module = "_noon_native")]
pub(crate) struct AudioSource(pub(crate) AudioResource);

#[pymethods]
impl AudioSource {
    #[staticmethod]
    #[pyo3(name = "fromInterleaved")]
    fn from_interleaved(
        py: Python<'_>,
        samples: PyBuffer<f32>,
        sample_rate: u32,
        channels: u16,
    ) -> PyResult<Self> {
        AudioResource::from_interleaved(
            sample_rate,
            channels,
            samples.to_vec(py)?,
            DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
        )
        .map(Self)
        .map_err(noon::AuthoringError::from)
        .map_err(engine_error)
    }

    #[staticmethod]
    #[pyo3(name = "smallDiscovery")]
    fn small_discovery() -> PyResult<Self> {
        noon::integration::small_discovery_audio()
            .map(Self)
            .map_err(engine_error)
    }

    #[getter(sampleRate)]
    fn sample_rate(&self) -> u32 {
        self.0.sample_rate()
    }

    #[getter]
    fn channels(&self) -> u16 {
        self.0.channels()
    }

    #[getter]
    fn frames(&self) -> usize {
        self.0.frames()
    }
}

#[pyclass(skip_from_py_object, module = "_noon_native")]
pub(crate) struct ClipOptions(pub(crate) AudioClipOptions);

#[pymethods]
impl ClipOptions {
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (start_time, source_start_frame, source_end_frame, duration, looped, gain, fade_in, fade_out))]
    fn new(
        start_time: f64,
        source_start_frame: usize,
        source_end_frame: Option<usize>,
        duration: Option<f64>,
        looped: bool,
        gain: f64,
        fade_in: f64,
        fade_out: f64,
    ) -> Self {
        Self(AudioClipOptions {
            start_time,
            source_start_frame,
            source_end_frame,
            duration,
            looped,
            gain,
            fade_in,
            fade_out,
        })
    }

    fn free(&self) {}
}

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub(crate) struct AudioClip {
    pub(crate) store: Rc<RefCell<SemanticStore>>,
    pub(crate) node: SemanticNodeId,
}

impl AudioClip {
    fn definition(&self) -> PyResult<AudioClipDefinition> {
        self.store
            .borrow()
            .semantic_audio_clip_state(self.node)
            .map(|clip| *clip.definition())
            .map_err(engine_error)
    }
}

#[pymethods]
impl AudioClip {
    #[getter(startTime)]
    fn start_time(&self) -> PyResult<f64> {
        self.definition().map(|clip| clip.start_time())
    }

    #[getter(endTime)]
    fn end_time(&self) -> PyResult<f64> {
        self.definition().map(|clip| clip.end_time())
    }

    #[getter]
    fn duration(&self) -> PyResult<f64> {
        self.definition().map(|clip| clip.duration())
    }
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<AudioSource>()?;
    module.add_class::<ClipOptions>()?;
    module.add_class::<AudioClip>()
}
