//! Thin CPython projections of immutable effect values and attachment handles.
use crate::engine_error;
use noon::effects::{EffectParameterError, EffectRadius, Glow, GlowSource};
use noon::Color;
use noon_core::{ShaderEffect, ShaderValue};
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
    #[pyo3(name = "setShaderScalar")]
    pub fn set_shader_scalar(
        &mut self,
        key: &crate::shaders::ShaderParameterKey,
        value: f64,
    ) -> Result<(), PyErr> {
        self.shader_write(key, ShaderValue::Scalar(value))
    }
    #[pyo3(name = "setShaderVector2")]
    pub fn set_shader_vector2(
        &mut self,
        key: &crate::shaders::ShaderParameterKey,
        value: Vec<f64>,
    ) -> Result<(), PyErr> {
        self.shader_write(key, ShaderValue::Vector2(crate::shaders::pair(&value)?))
    }
    #[pyo3(name = "setShaderColor")]
    pub fn set_shader_color(
        &mut self,
        key: &crate::shaders::ShaderParameterKey,
        value: Vec<f64>,
    ) -> Result<(), PyErr> {
        self.shader_write(key, ShaderValue::Color(crate::shaders::color(&value)?))
    }
    #[pyo3(name = "setShaderBoolean")]
    pub fn set_shader_boolean(
        &mut self,
        key: &crate::shaders::ShaderParameterKey,
        value: bool,
    ) -> Result<(), PyErr> {
        self.shader_write(key, ShaderValue::Boolean(value))
    }
    #[pyo3(name = "setShaderUnsigned")]
    pub fn set_shader_unsigned(
        &mut self,
        key: &crate::shaders::ShaderParameterKey,
        value: f64,
    ) -> Result<(), PyErr> {
        self.shader_write(key, ShaderValue::Unsigned(crate::shaders::unsigned(value)?))
    }
    #[pyo3(name="setRadial", signature=(center,axes,roughness,frequency,rim_width,inner_color,outer_color,hole))]
    #[allow(clippy::too_many_arguments)]
    pub fn set_radial(
        &mut self,
        center: Vec<f64>,
        axes: Vec<f64>,
        roughness: Option<f64>,
        frequency: Option<f64>,
        rim_width: Option<f64>,
        inner_color: Vec<f64>,
        outer_color: Vec<f64>,
        hole: Option<bool>,
    ) -> Result<(), PyErr> {
        let value = noon::integration::radial_parameters(
            self.value.clone(),
            &center,
            &axes,
            roughness,
            frequency,
            rim_width,
            &inner_color,
            &outer_color,
            hole,
        )
        .map_err(engine_error)?;
        self.value = value;
        Ok(())
    }
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
    fn mask_value(&self) -> Result<&noon_core::EffectDefinition, PyErr> {
        self.value
            .is_visibility_mask()
            .then_some(&self.value)
            .ok_or_else(|| {
                engine_error(noon::AuthoringError::from(
                    EffectParameterError::UnsupportedParameter(
                        noon_core::EffectParameter::Progress,
                    ),
                ))
            })
    }
    fn radial_value(&self) -> Result<noon_core::RadialMask, PyErr> {
        self.value.as_radial_mask().ok_or_else(|| {
            engine_error(noon::AuthoringError::from(
                EffectParameterError::IncompatibleDefinition,
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
    fn shader(program: &crate::shaders::ShaderProgram, update: &EffectUpdate) -> PyResult<Self> {
        update
            .value
            .apply_to(ShaderEffect::new(program.value.clone()))
            .map(|value| Self {
                value: value.into(),
            })
            .map_err(noon::AuthoringError::from)
            .map_err(engine_error)
    }
    #[pyo3(name = "shaderProgram")]
    fn shader_program(&self) -> PyResult<crate::shaders::ShaderProgram> {
        self.value
            .as_shader()
            .map(|v| crate::shaders::ShaderProgram {
                value: v.program().clone(),
            })
            .ok_or_else(|| crate::shaders::invalid("expected a shader effect"))
    }
    #[pyo3(name = "shaderValue")]
    fn shader_value(
        &self,
        py: Python<'_>,
        key: &crate::shaders::ShaderParameterKey,
    ) -> PyResult<Py<PyAny>> {
        let effect = self
            .value
            .as_shader()
            .ok_or_else(|| crate::shaders::invalid("expected a shader effect"))?;
        if effect
            .program()
            .parameter(&key.value.definition().name)
            .as_ref()
            != Some(&key.value)
        {
            return Err(crate::shaders::invalid(
                noon_core::ShaderProgramError::WrongProgram,
            ));
        }
        Ok(match effect.values()[key.value.index()] {
            ShaderValue::Scalar(v) => vec![v].into_pyobject(py)?.into_any().unbind(),
            ShaderValue::Vector2(v) => v.to_vec().into_pyobject(py)?.into_any().unbind(),
            ShaderValue::Color(c) => vec![
                f64::from(c.red),
                f64::from(c.green),
                f64::from(c.blue),
                f64::from(c.alpha),
            ]
            .into_pyobject(py)?
            .into_any()
            .unbind(),
            ShaderValue::Boolean(v) => v.into_pyobject(py)?.to_owned().into_any().unbind(),
            ShaderValue::Unsigned(v) => v.into_pyobject(py)?.into_any().unbind(),
        })
    }

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
    #[pyo3(name="radialMask", signature=(update,edge=None))]
    pub fn radial_mask(
        update: &EffectUpdate,
        edge: Option<String>,
    ) -> Result<EffectDefinition, PyErr> {
        let edge = noon::integration::radial_edge(edge.as_deref()).map_err(engine_error)?;
        noon_core::RadialMask::default()
            .with_update(update.value.clone(), edge)
            .map(|value| Self {
                value: value.into(),
            })
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
        Ok(self
            .mask_value()?
            .visibility_progress()
            .expect("checked mask"))
    }
    #[getter]
    pub fn softness(&self) -> Result<f64, PyErr> {
        Ok(self
            .mask_value()?
            .visibility_softness()
            .expect("checked mask"))
    }
    #[getter]
    pub fn seed(&self) -> Result<u32, PyErr> {
        self.value.as_shader().map_or_else(
            || {
                self.mask_value()
                    .map(|v| v.visibility_seed().expect("checked mask"))
            },
            |v| Ok(v.seed()),
        )
    }

    #[getter(revealCenter)]
    pub fn reveal_center(&self) -> Result<Vec<f64>, PyErr> {
        Ok(self.radial_value()?.center().to_vec())
    }
    #[getter(revealAxes)]
    pub fn reveal_axes(&self) -> Result<Vec<f64>, PyErr> {
        Ok(self.radial_value()?.axes().to_vec())
    }
    #[getter(revealRoughness)]
    pub fn reveal_roughness(&self) -> Result<f64, PyErr> {
        Ok(self.radial_value()?.roughness())
    }
    #[getter(revealFrequency)]
    pub fn reveal_frequency(&self) -> Result<f64, PyErr> {
        Ok(self.radial_value()?.frequency())
    }
    #[getter(revealRimWidth)]
    pub fn reveal_rim_width(&self) -> Result<f64, PyErr> {
        Ok(self.radial_value()?.rim_width())
    }
    #[getter(revealHole)]
    pub fn reveal_hole(&self) -> Result<bool, PyErr> {
        Ok(self.radial_value()?.hole())
    }
    #[getter(revealInnerColor)]
    pub fn reveal_inner_color(&self) -> Result<Vec<f64>, PyErr> {
        Ok({
            let c = self.radial_value()?.inner_color();
            vec![c.red.into(), c.green.into(), c.blue.into(), c.alpha.into()]
        })
    }
    #[getter(revealOuterColor)]
    pub fn reveal_outer_color(&self) -> Result<Vec<f64>, PyErr> {
        Ok({
            let c = self.radial_value()?.outer_color();
            vec![c.red.into(), c.green.into(), c.blue.into(), c.alpha.into()]
        })
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
            noon::effects::EffectDefinition::RadialMask(_) => "radial_mask",
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

impl EffectUpdate {
    fn shader_write(
        &mut self,
        key: &crate::shaders::ShaderParameterKey,
        value: ShaderValue,
    ) -> Result<(), PyErr> {
        let update = self.value.clone().shader_parameter(&key.value, value);
        update
            .validate()
            .map_err(noon::AuthoringError::from)
            .map_err(engine_error)?;
        self.value = update;
        Ok(())
    }
}

/// Own the original shared transaction until actual device preparation succeeds.
#[pyclass(unsendable, module = "_noon_native")]
pub struct StagedEffectPublication {
    pub(crate) pending: Option<noon::StagedEffectPublication>,
    pub(crate) runtime: noon::integration::RuntimeIdentity,
    pub(crate) bindings: Vec<noon::Mobject>,
}
#[pymethods]
impl StagedEffectPublication {
    fn free(&mut self) {
        self.pending.take();
    }
}
