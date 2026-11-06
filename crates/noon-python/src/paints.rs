//! Immutable paint projections. Rust owns validation, sampling and interpolation.
use crate::engine_error;
use pyo3::prelude::*;
use std::sync::Arc;

#[pyclass(unsendable, module = "_noon_native")]
pub struct LinearGradient {
    pub(crate) value: Arc<noon::LinearGradient>,
}
#[pymethods]
impl LinearGradient {
    #[new]
    fn new(start: Vec<f64>, end: Vec<f64>, stops: Vec<f64>) -> PyResult<Self> {
        noon::integration::linear_gradient(&start, &end, &stops)
            .map(|value| Self { value })
            .map_err(engine_error)
    }
}

#[pyclass(unsendable, module = "_noon_native")]
pub struct RadialGradient {
    pub(crate) value: Arc<noon::RadialGradient>,
}
#[pymethods]
impl RadialGradient {
    #[new]
    fn new(center: Vec<f64>, radii: Vec<f64>, stops: Vec<f64>) -> PyResult<Self> {
        noon::integration::radial_gradient(&center, &radii, &stops)
            .map(|value| Self { value })
            .map_err(engine_error)
    }
}

#[pyclass(unsendable, module = "_noon_native")]
pub struct NoisePaint {
    pub(crate) value: Arc<noon::NoisePaint>,
}
#[pymethods]
impl NoisePaint {
    #[new]
    fn new(seed: f64, frequency: Vec<f64>, phase: Vec<f64>, colors: Vec<f64>) -> PyResult<Self> {
        noon::integration::noise_paint(seed, &frequency, &phase, &colors)
            .map(|value| Self { value })
            .map_err(engine_error)
    }
}

#[pyclass(unsendable, module = "_noon_native")]
pub struct HatchPaint {
    pub(crate) value: Arc<noon::HatchPaint>,
}
#[pymethods]
impl HatchPaint {
    #[new]
    fn new(
        spacing: f64,
        width: f64,
        angle: f64,
        phase: Vec<f64>,
        cross: bool,
        colors: Vec<f64>,
    ) -> PyResult<Self> {
        noon::integration::hatch_paint(spacing, width, angle, &phase, cross, &colors)
            .map(|value| Self { value })
            .map_err(engine_error)
    }
}

#[pyclass(unsendable, module = "_noon_native")]
pub struct ImageMapping {
    pub(crate) value: noon::ImageMapping,
}
#[pymethods]
impl ImageMapping {
    #[new]
    fn new(
        fit: &str,
        alignment: Vec<f64>,
        address: &str,
        translation: Vec<f64>,
        scale: Vec<f64>,
        rotation: f64,
    ) -> PyResult<Self> {
        noon::integration::image_mapping(fit, &alignment, address, &translation, &scale, rotation)
            .map(|value| Self { value })
            .map_err(engine_error)
    }
}
#[pyclass(unsendable, module = "_noon_native")]
pub struct ImagePaintOptions {
    pub(crate) options: noon::ImagePaintOptions,
}
#[pymethods]
impl ImagePaintOptions {
    #[staticmethod]
    fn rgba8(width: u32, height: u32, pixels: &[u8]) -> PyResult<Self> {
        noon::ImagePaintOptions::rgba8(width, height, pixels)
            .map(|options| Self { options })
            .map_err(engine_error)
    }
    #[staticmethod]
    fn encoded(bytes: &[u8]) -> PyResult<Self> {
        #[cfg(feature = "image-decode")]
        {
            noon::ImagePaintOptions::encoded(bytes)
                .map(|options| Self { options })
                .map_err(engine_error)
        }
        #[cfg(not(feature = "image-decode"))]
        {
            let _ = bytes;
            Err(engine_error(noon::integration::AuthoringFailure::new(
                "unsupported_operation",
                "image.decoder_unavailable",
                "encoded image input requires the optional image-decode feature",
            )))
        }
    }
    #[staticmethod]
    #[pyo3(name = "fromImage")]
    fn from_image(image: &crate::mobject::MobjectHandle) -> PyResult<Self> {
        noon::ImagePaintOptions::from_image(&image.handle)
            .map(|options| Self { options })
            .map_err(engine_error)
    }
    fn copy(&self) -> Self {
        Self {
            options: self.options.clone(),
        }
    }
    #[pyo3(name = "setMapping")]
    fn mapping(&mut self, value: &ImageMapping) {
        self.options.set_mapping(value.value);
    }
    #[pyo3(name = "setSampling")]
    fn sampling(&mut self, value: &str) -> PyResult<()> {
        self.options
            .set_sampling(noon::integration::image_sampling(value).map_err(engine_error)?);
        Ok(())
    }
    fn free(&self) {}
}
