//! Optional Python/WASM value coercion over the checked Core program and keys.
use crate::engine_error;
use noon::integration::AuthoringFailure;
use noon_core::{
    ShaderCoordinates, ShaderDependencies, ShaderParameterDefinition,
    ShaderParameterKey as CoreShaderParameterKey, ShaderParameterType, ShaderUnit, ShaderValue,
    WgslEffectProgram,
};
use pyo3::prelude::*;

pub(crate) fn invalid(message: impl ToString) -> PyErr {
    engine_error(AuthoringFailure::new(
        "invalid_input",
        "shader.invalid_input",
        message,
    ))
}
fn unit(value: &str) -> Result<ShaderUnit, PyErr> {
    Ok(match value {
        "unitless" => ShaderUnit::Unitless,
        "normalized" => ShaderUnit::Normalized,
        "scene" => ShaderUnit::Scene,
        "pixels" => ShaderUnit::OutputPixels,
        "radians" => ShaderUnit::Radians,
        "seconds" => ShaderUnit::Seconds,
        _ => return Err(invalid("unknown shader parameter unit")),
    })
}
pub(crate) fn pair(value: &[f64]) -> Result<[f64; 2], PyErr> {
    value
        .try_into()
        .map_err(|_| invalid("expected two vector components"))
}
pub(crate) fn color(value: &[f64]) -> Result<noon_core::Color, PyErr> {
    noon::integration::effect_color(value)
        .map_err(engine_error)?
        .ok_or_else(|| invalid("expected four color components"))
}
pub(crate) fn unsigned(value: f64) -> Result<u32, PyErr> {
    if value.is_finite() && value.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&value) {
        Ok(value as u32)
    } else {
        Err(invalid("expected an integer in [0, 4294967295]"))
    }
}

#[pyclass(unsendable, module = "_noon_native")]
pub struct ShaderParameter {
    value: ShaderParameterDefinition,
}
impl ShaderParameter {
    fn from_definition(name: &str, kind: ShaderParameterType, default: ShaderValue) -> Self {
        Self {
            value: ShaderParameterDefinition {
                name: name.into(),
                kind,
                default,
            },
        }
    }
}
#[pymethods]
impl ShaderParameter {
    #[staticmethod]
    pub fn scalar(
        name: &str,
        default: f64,
        min: f64,
        max: f64,
        units: &str,
    ) -> Result<Self, PyErr> {
        Ok(Self::from_definition(
            name,
            ShaderParameterType::Scalar {
                range: min..=max,
                unit: unit(units)?,
            },
            ShaderValue::Scalar(default),
        ))
    }
    #[staticmethod]
    pub fn vector2(
        name: &str,
        default: Vec<f64>,
        min: Vec<f64>,
        max: Vec<f64>,
        units: &str,
    ) -> Result<Self, PyErr> {
        Ok(Self::from_definition(
            name,
            ShaderParameterType::Vector2 {
                min: pair(&min)?,
                max: pair(&max)?,
                unit: unit(units)?,
            },
            ShaderValue::Vector2(pair(&default)?),
        ))
    }
    #[staticmethod]
    pub fn color(name: &str, default: Vec<f64>) -> Result<Self, PyErr> {
        Ok(Self::from_definition(
            name,
            ShaderParameterType::Color,
            ShaderValue::Color(color(&default)?),
        ))
    }
    #[staticmethod]
    pub fn boolean(name: &str, default: bool) -> Self {
        Self::from_definition(
            name,
            ShaderParameterType::Boolean,
            ShaderValue::Boolean(default),
        )
    }
    #[staticmethod]
    pub fn unsigned(name: &str, default: f64, min: f64, max: f64) -> Result<Self, PyErr> {
        Ok(Self::from_definition(
            name,
            ShaderParameterType::Unsigned {
                range: unsigned(min)?..=unsigned(max)?,
            },
            ShaderValue::Unsigned(unsigned(default)?),
        ))
    }
}

/// Short-lived schema builder; prepared programs own the immutable checked copy.
#[pyclass(unsendable, module = "_noon_native")]
pub struct ShaderSchema {
    #[cfg_attr(not(feature = "wgsl-effects"), allow(dead_code))]
    dependencies: ShaderDependencies,
    parameters: Vec<ShaderParameterDefinition>,
}
#[pymethods]
impl ShaderSchema {
    #[new]
    pub fn new(coordinates: &str, authored_time: bool, seed: bool) -> Result<Self, PyErr> {
        let coordinates = match coordinates {
            "local" => ShaderCoordinates::LocalNormalized,
            "scene" => ShaderCoordinates::Scene,
            "pixels" => ShaderCoordinates::OutputPixels,
            _ => return Err(invalid("shader coordinates must be local, scene or pixels")),
        };
        Ok(Self {
            dependencies: ShaderDependencies {
                coordinates,
                authored_time,
                seed,
            },
            parameters: Vec::new(),
        })
    }
    #[pyo3(name = "addParameter")]
    pub fn add_parameter(&mut self, parameter: &ShaderParameter) {
        self.parameters.push(parameter.value.clone());
    }
    fn free(&self) {}
    pub fn prepare(&self, source: &str) -> Result<ShaderProgram, PyErr> {
        #[cfg(not(feature = "wgsl-effects"))]
        {
            let _ = source;
            Err(engine_error(AuthoringFailure::new(
                "unsupported_operation",
                "shader.preparation_unavailable",
                "build the native extension with wgsl-effects to prepare shader source",
            )))
        }
        #[cfg(feature = "wgsl-effects")]
        {
            WgslEffectProgram::prepare(
                noon_core::ShaderSchema {
                    profile: noon_core::ShaderProfile::PaintedPointwiseV1,
                    dependencies: self.dependencies,
                    parameters: self.parameters.clone().into_boxed_slice(),
                },
                source,
            )
            .map(|value| ShaderProgram { value })
            .map_err(invalid)
        }
    }
}
#[pyclass(unsendable, module = "_noon_native")]
pub struct ShaderProgram {
    pub(crate) value: WgslEffectProgram,
}
#[pymethods]
impl ShaderProgram {
    pub fn parameter(&self, name: &str) -> Result<ShaderParameterKey, PyErr> {
        self.value
            .parameter(name)
            .map(|value| ShaderParameterKey { value })
            .ok_or_else(|| invalid("shader parameter is absent from this program"))
    }
}
#[pyclass(unsendable, module = "_noon_native")]
pub struct ShaderParameterKey {
    pub(crate) value: CoreShaderParameterKey,
}
#[pymethods]
impl ShaderParameterKey {
    #[getter]
    pub fn kind(&self) -> String {
        match self.value.definition().kind {
            ShaderParameterType::Scalar { .. } => "scalar",
            ShaderParameterType::Vector2 { .. } => "vector2",
            ShaderParameterType::Color => "color",
            ShaderParameterType::Boolean => "boolean",
            ShaderParameterType::Unsigned { .. } => "unsigned",
        }
        .into()
    }
}
