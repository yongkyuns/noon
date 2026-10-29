//! Immutable author-supplied programs for the finite painted-pointwise profile.
//! Preparation checks the source and schema; it owns no scene, GPU or clock.
use std::{fmt, ops::RangeInclusive, sync::Arc};

use crate::Color;

/// Version one consumes one painted source, preserves its bounds, and returns
/// straight encoded RGB plus a coverage multiplier. The renderer associates
/// that result with source alpha before applying final scope opacity once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderProfile {
    PaintedPointwiseV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderCoordinates {
    /// Original object-local bounds mapped to [0, 1], +Y up. This is not
    /// normalized padded capture space or a screen-aligned bounding box.
    LocalNormalized,
    Scene,
    /// Final backing pixels, origin at the top left, +Y down.
    OutputPixels,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShaderDependencies {
    pub coordinates: ShaderCoordinates,
    /// Canonical authored seconds, supplied by Runtime, never wall time.
    pub authored_time: bool,
    /// An explicitly authored u32 seed; no implicit random state.
    pub seed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderUnit {
    Unitless,
    Normalized,
    Scene,
    OutputPixels,
    Radians,
    Seconds,
}

/// Colors and numeric vectors remain distinct semantic types even when their
/// lowered GPU representation could otherwise be interchangeable.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub enum ShaderValue {
    Scalar(f64),
    Vector2([f64; 2]),
    Color(Color),
    Boolean(bool),
    Unsigned(u32),
}

// WGSL bitcasts can observe signed zero. A same-valued-write fast path must not
// discard a change to the authored bits, including vector and color channels.
impl PartialEq for ShaderValue {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Self::Scalar(a), Self::Scalar(b)) => a.to_bits() == b.to_bits(),
            (Self::Vector2(a), Self::Vector2(b)) => a.map(f64::to_bits) == b.map(f64::to_bits),
            (Self::Color(a), Self::Color(b)) => {
                [a.red, a.green, a.blue, a.alpha].map(f32::to_bits)
                    == [b.red, b.green, b.blue, b.alpha].map(f32::to_bits)
            }
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::Unsigned(a), Self::Unsigned(b)) => a == b,
            _ => false,
        }
    }
}

/// Floating values interpolate; booleans and unsigned integers are discrete.
/// Author values stay f64. Admission additionally requires finite f32 lowering.
#[derive(Clone, Debug, PartialEq)]
pub enum ShaderParameterType {
    Scalar {
        range: RangeInclusive<f64>,
        unit: ShaderUnit,
    },
    Vector2 {
        min: [f64; 2],
        max: [f64; 2],
        unit: ShaderUnit,
    },
    Color,
    Boolean,
    Unsigned {
        range: RangeInclusive<u32>,
    },
}

impl ShaderParameterType {
    pub const fn is_continuous(&self) -> bool {
        matches!(
            self,
            Self::Scalar { .. } | Self::Vector2 { .. } | Self::Color
        )
    }
    #[cfg(feature = "wgsl-effects")]
    fn wgsl(&self) -> &'static str {
        match self {
            Self::Scalar { .. } => "f32",
            Self::Vector2 { .. } => "vec2<f32>",
            Self::Color => "vec4<f32>",
            Self::Boolean => "bool",
            Self::Unsigned { .. } => "u32",
        }
    }
    #[cfg(feature = "wgsl-effects")]
    fn valid(&self) -> bool {
        let range = |a: f64, b: f64| finite(a) && finite(b) && a <= b;
        match self {
            Self::Scalar { range: r, .. } => range(*r.start(), *r.end()),
            Self::Vector2 { min, max, .. } => (0..2).all(|a| range(min[a], max[a])),
            Self::Unsigned { range } => !range.is_empty(),
            Self::Color | Self::Boolean => true,
        }
    }
    pub fn accepts(&self, value: ShaderValue) -> bool {
        match (self, value) {
            (Self::Scalar { range, .. }, ShaderValue::Scalar(v)) => finite(v) && range.contains(&v),
            (Self::Vector2 { min, max, .. }, ShaderValue::Vector2(v)) => {
                (0..2).all(|a| finite(v[a]) && (min[a]..=max[a]).contains(&v[a]))
            }
            (Self::Color, ShaderValue::Color(v)) => [v.red, v.green, v.blue, v.alpha]
                .into_iter()
                .all(|c| c.is_finite() && (0.0..=1.0).contains(&c)),
            (Self::Boolean, ShaderValue::Boolean(_)) => true,
            (Self::Unsigned { range }, ShaderValue::Unsigned(v)) => range.contains(&v),
            _ => false,
        }
    }
}

fn finite(v: f64) -> bool {
    v.is_finite() && (v as f32).is_finite()
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShaderParameterDefinition {
    pub name: Arc<str>,
    pub kind: ShaderParameterType,
    pub default: ShaderValue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShaderSchema {
    pub profile: ShaderProfile,
    pub dependencies: ShaderDependencies,
    pub parameters: Box<[ShaderParameterDefinition]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShaderProgramError {
    Schema(&'static str),
    InvalidParameter(Arc<str>),
    WrongProgram,
    Wgsl(String),
    UnsupportedResource,
    Discard,
    UnsupportedStage,
    Signature,
}
impl fmt::Display for ShaderProgramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schema(message) => write!(f, "invalid effect shader schema: {message}"),
            Self::InvalidParameter(name) => write!(f, "invalid effect shader parameter: {name}"),
            Self::WrongProgram => f.write_str("shader parameter belongs to another program/schema"),
            Self::Wgsl(message) => write!(f, "invalid effect WGSL: {message}"),
            Self::UnsupportedResource => f.write_str("pointwise effects cannot declare bindings, mutable globals, overrides or entry points"),
            Self::Discard => f.write_str("pointwise effects return coverage instead of discarding fragments"),
            Self::UnsupportedStage => f.write_str("pointwise effects must be valid in a fragment stage"),
            Self::Signature => f.write_str("expected fn shade(NoonShaderInput, NoonShaderParameters) -> vec4<f32>"),
        }
    }
}
impl std::error::Error for ShaderProgramError {}

#[derive(Debug, PartialEq)]
struct Program {
    schema: ShaderSchema,
    source_start: usize,
    module: Arc<str>,
}

/// Checked immutable source/schema shared by attachment copies. GPU residency
/// is derived separately. Validating WGSL does not bound arbitrary shader cost;
/// this extension accepts trusted source, not sandboxed untrusted programs.
#[derive(Clone, Debug)]
pub struct WgslEffectProgram(Arc<Program>);
impl PartialEq for WgslEffectProgram {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

impl WgslEffectProgram {
    #[cfg(feature = "wgsl-effects")]
    pub fn prepare(
        schema: ShaderSchema,
        source: impl Into<Arc<str>>,
    ) -> Result<Self, ShaderProgramError> {
        let source = source.into();
        if source.len() > 64 * 1024 || schema.parameters.len() > 16 {
            return Err(ShaderProgramError::Schema(
                "at most 64 KiB source and 16 parameters",
            ));
        }
        let mut module =
            String::from("struct NoonShaderInput { color: vec4<f32>, position: vec2<f32>,\n");
        if schema.dependencies.authored_time {
            module.push_str("time: f32,\n");
        }
        if schema.dependencies.seed {
            module.push_str("seed: u32,\n");
        }
        module.push_str("}\nstruct NoonShaderParameters {\n");
        for (index, p) in schema.parameters.iter().enumerate() {
            let mut chars = p.name.chars();
            if p.name.len() > 64
                || !chars.next().is_some_and(|c| c.is_ascii_alphabetic())
                || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
                || p.name.starts_with("noon_")
                || schema.parameters[..index]
                    .iter()
                    .any(|other| p.name == other.name)
                || !p.kind.valid()
                || !p.kind.accepts(p.default)
            {
                return Err(ShaderProgramError::InvalidParameter(p.name.clone()));
            }
            use fmt::Write;
            writeln!(module, "{}: {},", p.name, p.kind.wgsl()).expect("writing a String");
        }
        if schema.parameters.is_empty() {
            module.push_str("noon_unused: u32,\n");
        }
        module.push_str("}\n");
        let source_start = module.len();
        module.push_str(&source);
        validate_wgsl(&module)?;
        Ok(Self(Arc::new(Program {
            schema,
            source_start,
            module: module.into(),
        })))
    }
    pub fn schema(&self) -> &ShaderSchema {
        &self.0.schema
    }
    pub fn source(&self) -> &str {
        &self.0.module[self.0.source_start..]
    }
    /// Validated function module, including generated typed declarations. This
    /// content is the GPU program cache key; attachment values are not included.
    pub fn module(&self) -> &Arc<str> {
        &self.0.module
    }
    pub fn parameter(&self, name: &str) -> Option<ShaderParameterKey> {
        self.schema()
            .parameters
            .iter()
            .position(|p| p.name.as_ref() == name)
            .map(|index| ShaderParameterKey {
                program: self.clone(),
                index,
            })
    }
}

#[cfg(feature = "wgsl-effects")]
fn validate_wgsl(source: &str) -> Result<(), ShaderProgramError> {
    let module = naga::front::wgsl::parse_str(source)
        .map_err(|e| ShaderProgramError::Wgsl(e.emit_to_string(source)))?;
    if !module.global_variables.is_empty()
        || !module.overrides.is_empty()
        || !module.entry_points.is_empty()
    {
        return Err(ShaderProgramError::UnsupportedResource);
    }
    if module
        .functions
        .iter()
        .any(|(_, function)| discards(&function.body))
    {
        return Err(ShaderProgramError::Discard);
    }
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .map_err(|e| ShaderProgramError::Wgsl(e.emit_to_string(source)))?;
    let (function_id, function) = module
        .functions
        .iter()
        .find(|(_, f)| f.name.as_deref() == Some("shade"))
        .ok_or(ShaderProgramError::Signature)?;
    if !info[function_id]
        .available_stages
        .contains(naga::valid::ShaderStages::FRAGMENT)
    {
        return Err(ShaderProgramError::UnsupportedStage);
    }
    if function.arguments.len() != 2
        || module.types[function.arguments[0].ty].name.as_deref() != Some("NoonShaderInput")
        || module.types[function.arguments[1].ty].name.as_deref() != Some("NoonShaderParameters")
        || function.result.as_ref().is_none_or(|r| {
            !matches!(
                module.types[r.ty].inner,
                naga::TypeInner::Vector {
                    size: naga::VectorSize::Quad,
                    scalar: naga::Scalar {
                        kind: naga::ScalarKind::Float,
                        width: 4
                    }
                }
            )
        })
    {
        return Err(ShaderProgramError::Signature);
    }
    Ok(())
}

// Naga's `may_kill` reports divergent termination for uniformity analysis; it
// intentionally omits uniform discard. This profile forbids either form.
#[cfg(feature = "wgsl-effects")]
fn discards(block: &naga::Block) -> bool {
    block.iter().any(|statement| match statement {
        naga::Statement::Kill => true,
        naga::Statement::Block(block) => discards(block),
        naga::Statement::If { accept, reject, .. } => discards(accept) || discards(reject),
        naga::Statement::Switch { cases, .. } => cases.iter().any(|case| discards(&case.body)),
        naga::Statement::Loop {
            body, continuing, ..
        } => discards(body) || discards(continuing),
        _ => false,
    })
}

/// A schema-bound parameter address, not a semantic object or ID allocator.
/// A key cannot silently address another program after source/schema replacement.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderParameterKey {
    program: WgslEffectProgram,
    index: usize,
}
impl ShaderParameterKey {
    pub fn definition(&self) -> &ShaderParameterDefinition {
        &self.program.schema().parameters[self.index]
    }
    pub fn index(&self) -> usize {
        self.index
    }
}

/// Inert immutable parameter values. Updates validate before replacing this
/// value; the semantic attachment/Runtime remain the eventual mutable owners.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderEffect {
    program: WgslEffectProgram,
    values: Arc<[ShaderValue]>,
}
impl ShaderEffect {
    pub fn new(program: WgslEffectProgram) -> Self {
        let values = program
            .schema()
            .parameters
            .iter()
            .map(|p| p.default)
            .collect();
        Self { program, values }
    }
    pub fn program(&self) -> &WgslEffectProgram {
        &self.program
    }
    pub fn values(&self) -> &[ShaderValue] {
        &self.values
    }
    pub fn with_parameter(
        &self,
        key: &ShaderParameterKey,
        value: ShaderValue,
    ) -> Result<Self, ShaderProgramError> {
        if self.program != key.program {
            return Err(ShaderProgramError::WrongProgram);
        }
        if !key.definition().kind.accepts(value) {
            return Err(ShaderProgramError::InvalidParameter(
                key.definition().name.clone(),
            ));
        }
        if self.values[key.index] == value {
            return Ok(self.clone());
        }
        let mut next = self.clone();
        Arc::make_mut(&mut next.values)[key.index] = value;
        Ok(next)
    }
}

impl crate::object_state::effect_value_sealed::Sealed for ShaderEffect {}
impl crate::EffectValue for ShaderEffect {
    fn apply_effect_update(
        mut self,
        update: &crate::EffectUpdate,
    ) -> Result<Self, crate::EffectParameterError> {
        use crate::{EffectParameter, EffectParameterError};
        update.validate()?;
        for parameter in update.parameters() {
            if !matches!(parameter, EffectParameter::Shader(_)) {
                return Err(EffectParameterError::UnsupportedParameter(parameter));
            }
        }
        // Check the complete patch before making any values writable.
        for write in update.shader_parameters() {
            if write.key.program != self.program {
                return Err(EffectParameterError::ShaderProgramMismatch);
            }
        }
        if update
            .shader_parameters()
            .iter()
            .any(|w| self.values[w.key.index] != w.value)
        {
            let values = Arc::make_mut(&mut self.values);
            for write in update.shader_parameters() {
                values[write.key.index] = write.value;
            }
        }
        Ok(self)
    }
    fn validate_effect_value_update(
        &self,
        target: &Self,
    ) -> Result<(), crate::EffectParameterError> {
        if self.program != target.program {
            return Err(crate::EffectParameterError::ShaderProgramMismatch);
        }
        Ok(())
    }
    fn validate_effect_transition(&self, target: &Self) -> Result<(), crate::EffectParameterError> {
        use crate::{EffectParameter, EffectParameterError};
        self.validate_effect_value_update(target)?;
        for (index, parameter) in self.program.schema().parameters.iter().enumerate() {
            if !parameter.kind.is_continuous() && self.values[index] != target.values[index] {
                return Err(EffectParameterError::DiscreteTransition(
                    EffectParameter::Shader(index as u8),
                ));
            }
        }
        Ok(())
    }
    fn sample_effect_parameters(
        &self,
        target: &Self,
        request: &crate::EffectUpdate,
        alpha: f64,
    ) -> Result<crate::EffectUpdate, crate::EffectParameterError> {
        let mut result = crate::EffectUpdate::default();
        for write in request.shader_parameters() {
            let index = write.key.index;
            let value = self.values[index]
                .interpolate(target.values[index], alpha)
                .ok_or(crate::EffectParameterError::InvalidShaderParameter(
                    index as u8,
                ))?;
            result = result.shader_parameter(&write.key, value);
        }
        result.validate()?;
        Ok(result)
    }
}

impl ShaderValue {
    /// Same typed value at a canonical unit progress. Discrete changes do not
    /// acquire an interpolation rule merely because their storage is numeric.
    pub(crate) fn interpolate(self, target: Self, alpha: f64) -> Option<Self> {
        if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
            return None;
        }
        if alpha == 0.0 {
            return Some(self);
        }
        if alpha == 1.0 {
            return Some(target);
        }
        let scalar = |a: f64, b: f64| a * (1.0 - alpha) + b * alpha;
        match (self, target) {
            (Self::Scalar(a), Self::Scalar(b)) => Some(Self::Scalar(scalar(a, b))),
            (Self::Vector2(a), Self::Vector2(b)) => {
                Some(Self::Vector2([scalar(a[0], b[0]), scalar(a[1], b[1])]))
            }
            (Self::Color(a), Self::Color(b)) => {
                let c = |a: f32, b: f32| scalar(f64::from(a), f64::from(b)) as f32;
                Some(Self::Color(Color::rgba(
                    c(a.red, b.red),
                    c(a.green, b.green),
                    c(a.blue, b.blue),
                    c(a.alpha, b.alpha),
                )))
            }
            (Self::Boolean(a), Self::Boolean(b)) if a == b => Some(self),
            (Self::Unsigned(a), Self::Unsigned(b)) if a == b => Some(self),
            _ => None,
        }
    }
}

#[cfg(all(test, feature = "wgsl-effects"))]
mod tests;
