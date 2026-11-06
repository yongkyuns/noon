//! Thin CPython projections of immutable effect values and attachment handles.
use crate::engine_error;
use noon::effects::{EffectParameterError, EffectRadius, Glow, GlowSource};
use noon::Color;
use pyo3::prelude::*;

#[pyclass(unsendable, module = "_noon_native")]
pub struct EffectUpdate {
    pub(crate) value: noon::effects::EffectUpdate,
}
#[pyclass(unsendable, module = "_noon_native")]
pub struct EffectDefinition {
    pub(crate) value: noon::effects::EffectDefinition,
}
#[pyclass(unsendable, module = "_noon_native")]
pub struct EffectHandle {
    pub(crate) handle: noon::effects::EffectHandle,
}
#[pymethods]
#[allow(clippy::too_many_arguments)]
impl EffectUpdate {
    #[pyo3(name = "setThreshold")]
    pub fn set_threshold(&mut self, threshold: f64) -> Result<(), PyErr> {
        let update = self.value.clone().threshold(threshold);
        update
            .validate()
            .map_err(noon::AuthoringError::from)
            .map_err(engine_error)?;
        self.value = update;
        Ok(())
    }

    #[pyo3(name = "setSize")]
    pub fn set_size(&mut self, size: f64, pixels: bool) -> Result<(), PyErr> {
        let value = self.value.clone().size(if pixels {
            EffectRadius::Pixels(size)
        } else {
            EffectRadius::Scene(size)
        });
        value
            .validate()
            .map_err(noon::AuthoringError::from)
            .map_err(engine_error)?;
        self.value = value;
        Ok(())
    }
    #[new]
    #[pyo3(signature=(color, radius, pixels, intensity, source, offset, offset_pixels, progress, softness, seed))]
    pub fn new(
        color: Vec<f64>,
        radius: Option<f64>,
        pixels: bool,
        intensity: Option<f64>,
        source: Option<String>,
        offset: Vec<f64>,
        offset_pixels: bool,
        progress: Option<f64>,
        softness: Option<f64>,
        seed: Option<f64>,
    ) -> Result<EffectUpdate, PyErr> {
        noon::integration::effect_update(
            &color,
            radius,
            pixels,
            intensity,
            source.as_deref(),
            &offset,
            offset_pixels,
        )
        .and_then(|value| noon::integration::mask_parameters(value, progress, softness, seed))
        .map(|value| Self { value })
        .map_err(engine_error)
    }
    fn free(&self) {}
}

impl EffectDefinition {
    fn pixelate_value(&self) -> Result<noon_core::Pixelate, PyErr> {
        self.value.as_pixelate().ok_or_else(|| {
            engine_error(noon::AuthoringError::from(
                EffectParameterError::UnsupportedParameter(noon_core::EffectParameter::Size),
            ))
        })
    }
    fn radius_value(&self) -> Result<EffectRadius, PyErr> {
        self.value.radius().ok_or_else(|| {
            engine_error(noon::AuthoringError::from(
                EffectParameterError::UnsupportedParameter(noon_core::EffectParameter::Radius),
            ))
        })
    }
    fn mask_value(&self) -> Result<noon_core::NoiseDissolve, PyErr> {
        self.value.as_noise_dissolve().ok_or_else(|| {
            engine_error(noon::AuthoringError::from(
                EffectParameterError::UnsupportedParameter(noon_core::EffectParameter::Progress),
            ))
        })
    }
    fn glow_value(&self) -> Result<Glow, PyErr> {
        self.value.as_glow().ok_or_else(|| {
            engine_error(noon::AuthoringError::from(
                EffectParameterError::UnsupportedParameter(noon_core::EffectParameter::Color),
            ))
        })
    }
    fn color_value(&self) -> Result<Color, PyErr> {
        self.value.color().ok_or_else(|| {
            engine_error(noon::AuthoringError::from(
                EffectParameterError::UnsupportedParameter(noon_core::EffectParameter::Color),
            ))
        })
    }
    fn shadow_value(&self) -> Result<noon::effects::Shadow, PyErr> {
        self.value.as_shadow().ok_or_else(|| {
            engine_error(noon::AuthoringError::from(
                EffectParameterError::UnsupportedParameter(noon_core::EffectParameter::Offset),
            ))
        })
    }
    pub(crate) fn definition(&self) -> noon::effects::EffectDefinition {
        self.value.clone()
    }
}

#[pymethods]
impl EffectDefinition {
    #[staticmethod]
    #[pyo3(name = "bloom")]
    pub fn bloom(update: &EffectUpdate) -> Result<EffectDefinition, PyErr> {
        noon_core::Bloom::new(update.value.clone())
            .map(|value| Self {
                value: value.into(),
            })
            .map_err(noon::AuthoringError::from)
            .map_err(engine_error)
    }
    #[getter]
    pub fn threshold(&self) -> Result<f64, PyErr> {
        self.value
            .as_bloom()
            .map(|value| value.threshold())
            .ok_or_else(|| {
                engine_error(noon::AuthoringError::from(
                    EffectParameterError::UnsupportedParameter(
                        noon_core::EffectParameter::Threshold,
                    ),
                ))
            })
    }
    #[staticmethod]
    #[pyo3(name = "glow")]
    pub fn glow(update: &EffectUpdate) -> Result<EffectDefinition, PyErr> {
        Glow::new(update.value.clone())
            .map(|value| Self {
                value: value.into(),
            })
            .map_err(noon::AuthoringError::from)
            .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name = "pixelate")]
    pub fn pixelate(update: &EffectUpdate) -> Result<EffectDefinition, PyErr> {
        noon_core::Pixelate::new(update.value.clone())
            .map(|value| Self {
                value: value.into(),
            })
            .map_err(noon::AuthoringError::from)
            .map_err(engine_error)
    }
    #[getter]
    pub fn size(&self) -> Result<f64, PyErr> {
        Ok(self.pixelate_value()?.size().value())
    }
    #[getter(sizePixels)]
    pub fn size_pixels(&self) -> Result<bool, PyErr> {
        Ok(matches!(
            self.pixelate_value()?.size(),
            EffectRadius::Pixels(_)
        ))
    }
    #[staticmethod]
    #[pyo3(name = "blur")]
    pub fn blur(update: &EffectUpdate) -> Result<EffectDefinition, PyErr> {
        noon::effects::Blur::new(update.value.clone())
            .map(|value| Self {
                value: value.into(),
            })
            .map_err(noon::AuthoringError::from)
            .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name = "backdropBlur")]
    pub fn backdrop_blur(update: &EffectUpdate) -> Result<EffectDefinition, PyErr> {
        noon::effects::BackdropBlur::new(update.value.clone())
            .map(|value| Self {
                value: value.into(),
            })
            .map_err(noon::AuthoringError::from)
            .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name = "shadow")]
    pub fn shadow(update: &EffectUpdate) -> Result<EffectDefinition, PyErr> {
        noon::effects::Shadow::new(update.value.clone())
            .map(|v| Self { value: v.into() })
            .map_err(noon::AuthoringError::from)
            .map_err(engine_error)
    }
    #[staticmethod]
    #[pyo3(name = "noiseDissolve")]
    pub fn noise_dissolve(update: &EffectUpdate) -> Result<EffectDefinition, PyErr> {
        update
            .value
            .apply_to(noon_core::NoiseDissolve::default())
            .map(|value| Self {
                value: value.into(),
            })
            .map_err(noon::AuthoringError::from)
            .map_err(engine_error)
    }
    #[getter]
    pub fn progress(&self) -> Result<f64, PyErr> {
        Ok(self.mask_value()?.progress())
    }
    #[getter]
    pub fn softness(&self) -> Result<f64, PyErr> {
        Ok(self.mask_value()?.softness())
    }
    #[getter]
    pub fn seed(&self) -> Result<u32, PyErr> {
        self.value
            .as_shader()
            .map_or_else(|| self.mask_value().map(|v| v.seed()), |v| Ok(v.seed()))
    }

    #[getter(offsetX)]
    pub fn offset_x(&self) -> Result<f64, PyErr> {
        Ok(self.shadow_value()?.offset().value()[0])
    }
    #[getter(offsetY)]
    pub fn offset_y(&self) -> Result<f64, PyErr> {
        Ok(self.shadow_value()?.offset().value()[1])
    }
    #[getter(offsetPixels)]
    pub fn offset_pixels(&self) -> Result<bool, PyErr> {
        Ok(matches!(
            self.shadow_value()?.offset(),
            noon_core::EffectOffset::Pixels(_)
        ))
    }
    #[getter]
    pub fn kind(&self) -> String {
        match self.value {
            noon::effects::EffectDefinition::Bloom(_) => "bloom",
            noon::effects::EffectDefinition::Shader(_) => "shader",
            noon::effects::EffectDefinition::Pixelate(_) => "pixelate",
            noon::effects::EffectDefinition::Glow(_) => "glow",
            noon::effects::EffectDefinition::Blur(_) => "blur",
            noon::effects::EffectDefinition::BackdropBlur(_) => "backdrop_blur",
            noon::effects::EffectDefinition::Shadow(_) => "shadow",
            noon::effects::EffectDefinition::NoiseDissolve(_) => "noise_dissolve",
        }
        .into()
    }
    #[getter]
    pub fn red(&self) -> Result<f64, PyErr> {
        Ok(f64::from(self.color_value()?.red))
    }
    #[getter]
    pub fn green(&self) -> Result<f64, PyErr> {
        Ok(f64::from(self.color_value()?.green))
    }
    #[getter]
    pub fn blue(&self) -> Result<f64, PyErr> {
        Ok(f64::from(self.color_value()?.blue))
    }
    #[getter]
    pub fn alpha(&self) -> Result<f64, PyErr> {
        Ok(f64::from(self.color_value()?.alpha))
    }
    #[getter]
    pub fn radius(&self) -> Result<f64, PyErr> {
        Ok(self.radius_value()?.value())
    }
    #[getter]
    pub fn pixels(&self) -> Result<bool, PyErr> {
        Ok(matches!(self.radius_value()?, EffectRadius::Pixels(_)))
    }
    #[getter]
    pub fn intensity(&self) -> Result<f64, PyErr> {
        if let Some(bloom) = self.value.as_bloom() {
            return Ok(bloom.intensity());
        }
        Ok(self.glow_value()?.intensity())
    }
    #[getter]
    pub fn source(&self) -> Result<String, PyErr> {
        Ok(match self.glow_value()?.source() {
            GlowSource::Painted => "painted",
            GlowSource::Silhouette => "silhouette",
        }
        .into())
    }
}

#[pymethods]
impl EffectHandle {
    #[pyo3(name = "authoredDefinition")]
    pub fn authored_definition(&self) -> Result<EffectDefinition, PyErr> {
        let value = self.handle.authored_definition().map_err(engine_error)?;
        Ok(EffectDefinition { value })
    }
}
