//! Optional Python/WASM value coercion over the checked Core program and keys.
use super::wasm::{WasmEffectDefinition, WasmEffectUpdate};
use crate::{authoring_error::js_error, AuthoringFailure};
use noon_core::{
    ShaderCoordinates, ShaderDependencies, ShaderEffect, ShaderParameterDefinition,
    ShaderParameterKey, ShaderParameterType, ShaderProfile, ShaderSchema, ShaderUnit, ShaderValue,
    WgslEffectProgram,
};
use wasm_bindgen::prelude::*;

fn invalid(message: impl ToString) -> JsValue {
    js_error(AuthoringFailure::new(
        "invalid_input",
        "shader.invalid_input",
        message,
    ))
}
fn unit(value: &str) -> Result<ShaderUnit, JsValue> {
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
fn pair(value: &[f64]) -> Result<[f64; 2], JsValue> {
    value
        .try_into()
        .map_err(|_| invalid("expected two vector components"))
}
fn color(value: &[f64]) -> Result<noon_core::Color, JsValue> {
    super::effect_color(value)
        .map_err(js_error)?
        .ok_or_else(|| invalid("expected four color components"))
}
fn unsigned(value: f64) -> Result<u32, JsValue> {
    if value.is_finite() && value.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&value) {
        Ok(value as u32)
    } else {
        Err(invalid("expected an integer in [0, 4294967295]"))
    }
}

#[wasm_bindgen]
pub struct WasmShaderParameter {
    value: ShaderParameterDefinition,
}
impl WasmShaderParameter {
    fn new(name: &str, kind: ShaderParameterType, default: ShaderValue) -> Self {
        Self {
            value: ShaderParameterDefinition {
                name: name.into(),
                kind,
                default,
            },
        }
    }
}
#[wasm_bindgen]
impl WasmShaderParameter {
    pub fn scalar(
        name: &str,
        default: f64,
        min: f64,
        max: f64,
        units: &str,
    ) -> Result<Self, JsValue> {
        Ok(Self::new(
            name,
            ShaderParameterType::Scalar {
                range: min..=max,
                unit: unit(units)?,
            },
            ShaderValue::Scalar(default),
        ))
    }
    pub fn vector2(
        name: &str,
        default: &[f64],
        min: &[f64],
        max: &[f64],
        units: &str,
    ) -> Result<Self, JsValue> {
        Ok(Self::new(
            name,
            ShaderParameterType::Vector2 {
                min: pair(min)?,
                max: pair(max)?,
                unit: unit(units)?,
            },
            ShaderValue::Vector2(pair(default)?),
        ))
    }
    pub fn color(name: &str, default: &[f64]) -> Result<Self, JsValue> {
        Ok(Self::new(
            name,
            ShaderParameterType::Color,
            ShaderValue::Color(color(default)?),
        ))
    }
    pub fn boolean(name: &str, default: bool) -> Self {
        Self::new(
            name,
            ShaderParameterType::Boolean,
            ShaderValue::Boolean(default),
        )
    }
    pub fn unsigned(name: &str, default: f64, min: f64, max: f64) -> Result<Self, JsValue> {
        Ok(Self::new(
            name,
            ShaderParameterType::Unsigned {
                range: unsigned(min)?..=unsigned(max)?,
            },
            ShaderValue::Unsigned(unsigned(default)?),
        ))
    }
}

/// Short-lived schema builder; prepared programs own the immutable checked copy.
#[wasm_bindgen]
pub struct WasmShaderSchema {
    dependencies: ShaderDependencies,
    parameters: Vec<ShaderParameterDefinition>,
}
#[wasm_bindgen]
impl WasmShaderSchema {
    #[wasm_bindgen(constructor)]
    pub fn new(coordinates: &str, authored_time: bool, seed: bool) -> Result<Self, JsValue> {
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
    #[wasm_bindgen(js_name = addParameter)]
    pub fn add_parameter(&mut self, parameter: &WasmShaderParameter) {
        self.parameters.push(parameter.value.clone());
    }
    pub fn prepare(&self, source: &str) -> Result<WasmShaderProgram, JsValue> {
        WgslEffectProgram::prepare(
            ShaderSchema {
                profile: ShaderProfile::PaintedPointwiseV1,
                dependencies: self.dependencies,
                parameters: self.parameters.clone().into_boxed_slice(),
            },
            source,
        )
        .map(|value| WasmShaderProgram { value })
        .map_err(invalid)
    }
}
#[wasm_bindgen]
pub struct WasmShaderProgram {
    value: WgslEffectProgram,
}
#[wasm_bindgen]
impl WasmShaderProgram {
    pub fn parameter(&self, name: &str) -> Result<WasmShaderParameterKey, JsValue> {
        self.value
            .parameter(name)
            .map(|value| WasmShaderParameterKey { value })
            .ok_or_else(|| invalid("shader parameter is absent from this program"))
    }
}
#[wasm_bindgen]
pub struct WasmShaderParameterKey {
    value: ShaderParameterKey,
}
#[wasm_bindgen]
impl WasmShaderParameterKey {
    #[wasm_bindgen(getter)]
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
impl WasmEffectUpdate {
    fn shader_write(
        &mut self,
        key: &WasmShaderParameterKey,
        value: ShaderValue,
    ) -> Result<(), JsValue> {
        let update = self.value.clone().shader_parameter(&key.value, value);
        update
            .validate()
            .map_err(noon::AuthoringError::from)
            .map_err(js_error)?;
        self.value = update;
        Ok(())
    }
}
#[wasm_bindgen]
impl WasmEffectUpdate {
    #[wasm_bindgen(js_name = setShaderScalar)]
    pub fn set_shader_scalar(
        &mut self,
        key: &WasmShaderParameterKey,
        value: f64,
    ) -> Result<(), JsValue> {
        self.shader_write(key, ShaderValue::Scalar(value))
    }
    #[wasm_bindgen(js_name = setShaderVector2)]
    pub fn set_shader_vector2(
        &mut self,
        key: &WasmShaderParameterKey,
        value: &[f64],
    ) -> Result<(), JsValue> {
        self.shader_write(key, ShaderValue::Vector2(pair(value)?))
    }
    #[wasm_bindgen(js_name = setShaderColor)]
    pub fn set_shader_color(
        &mut self,
        key: &WasmShaderParameterKey,
        value: &[f64],
    ) -> Result<(), JsValue> {
        self.shader_write(key, ShaderValue::Color(color(value)?))
    }
    #[wasm_bindgen(js_name = setShaderBoolean)]
    pub fn set_shader_boolean(
        &mut self,
        key: &WasmShaderParameterKey,
        value: bool,
    ) -> Result<(), JsValue> {
        self.shader_write(key, ShaderValue::Boolean(value))
    }
    #[wasm_bindgen(js_name = setShaderUnsigned)]
    pub fn set_shader_unsigned(
        &mut self,
        key: &WasmShaderParameterKey,
        value: f64,
    ) -> Result<(), JsValue> {
        self.shader_write(key, ShaderValue::Unsigned(unsigned(value)?))
    }
}
#[wasm_bindgen]
impl WasmEffectDefinition {
    pub fn shader(program: &WasmShaderProgram, update: &WasmEffectUpdate) -> Result<Self, JsValue> {
        update
            .value
            .apply_to(ShaderEffect::new(program.value.clone()))
            .map(|v| Self { value: v.into() })
            .map_err(noon::AuthoringError::from)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = shaderProgram)]
    pub fn shader_program(&self) -> Result<WasmShaderProgram, JsValue> {
        self.value
            .as_shader()
            .map(|v| WasmShaderProgram {
                value: v.program().clone(),
            })
            .ok_or_else(|| invalid("expected a shader effect"))
    }
    #[wasm_bindgen(js_name = shaderValue)]
    pub fn shader_value(&self, key: &WasmShaderParameterKey) -> Result<JsValue, JsValue> {
        let effect = self
            .value
            .as_shader()
            .ok_or_else(|| invalid("expected a shader effect"))?;
        if effect
            .program()
            .parameter(&key.value.definition().name)
            .as_ref()
            != Some(&key.value)
        {
            return Err(invalid(noon_core::ShaderProgramError::WrongProgram));
        }
        Ok(match effect.values()[key.value.index()] {
            // Pyodide converts integral JS Numbers to Python ints, losing -0.
            // Typed arrays expose a double buffer and preserve authored bits.
            ShaderValue::Scalar(v) => js_sys::Float64Array::from(&[v][..]).into(),
            ShaderValue::Vector2(v) => js_sys::Float64Array::from(&v[..]).into(),
            ShaderValue::Color(c) => js_sys::Float64Array::from(
                &[
                    f64::from(c.red),
                    f64::from(c.green),
                    f64::from(c.blue),
                    f64::from(c.alpha),
                ][..],
            )
            .into(),
            ShaderValue::Boolean(v) => JsValue::from_bool(v),
            ShaderValue::Unsigned(v) => JsValue::from_f64(f64::from(v)),
        })
    }
}
