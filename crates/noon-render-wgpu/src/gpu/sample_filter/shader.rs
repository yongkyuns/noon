//! GPU specialization of the existing sampled-image pass. Programs are prepared
//! fallibly before publication; ordinary parameter samples only pack uniforms.
use crate::gpu::{GpuLease, WeakGpuLease};
use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use noon_core::{
    ShaderCoordinates, ShaderEffect, ShaderParameterType, ShaderValue, WgslEffectProgram,
};

use super::{
    EffectPrepareError, EffectRasterStats, GaussianBlurCapture, Program, SampleFilter,
    SampleParameters, SampleScope,
};

/// Derived coordinate maps for the caller's padded capture. Local coordinates
/// refer to the original object's normalized box, +Y up, including after rotation.
/// A composed source without a qualified local map supplies None and rejects
/// LocalNormalized programs. Scene and output coordinates remain independent.
#[derive(Clone, Copy, Debug)]
pub struct ShaderSampleContext {
    pub local_from_capture: Option<[[f64; 3]; 2]>,
    pub scene_from_capture: [[f64; 3]; 2],
    pub output_origin: [f64; 2],
    pub authored_time: f64,
}

/// Disposable device-specific program. A publication/preparation lease keeps it
/// alive until scopes borrow it; the shared filter retains only weak cache keys.
#[derive(Debug)]
pub struct PreparedShaderProgram {
    program: Program,
    source: WgslEffectProgram,
    device: wgpu::Device,
}

impl PreparedShaderProgram {
    pub(in crate::gpu) fn matches(
        &self,
        device: &wgpu::Device,
        source: &WgslEffectProgram,
    ) -> bool {
        self.device == *device && self.source.module() == source.module()
    }
}

impl ShaderSampleContext {
    pub(in crate::gpu) fn for_capture(mut self, origin: [i32; 2]) -> Self {
        let translate = |rows: [[f64; 3]; 2]| {
            rows.map(|mut r| {
                r[2] += r[0] * f64::from(origin[0]) + r[1] * f64::from(origin[1]);
                r
            })
        };
        self.local_from_capture = self.local_from_capture.map(translate);
        self.scene_from_capture = translate(self.scene_from_capture);
        for (value, origin) in self.output_origin.iter_mut().zip(origin) {
            *value += f64::from(origin);
        }
        self
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(super) struct ShaderUniform {
    position: [[f32; 4]; 2],
    control: [f32; 4],
    integers: [u32; 4],
    parameters: [[u32; 4]; 16],
}

#[derive(Debug)]
pub(super) struct ShaderScope {
    program: GpuLease<PreparedShaderProgram>,
    uniform: ShaderUniform,
}

impl ShaderUniform {
    pub(super) fn new(
        definition: &ShaderEffect,
        extent: [u32; 2],
        context: Option<&ShaderSampleContext>,
        scope_opacity: f32,
    ) -> Result<Self, EffectPrepareError> {
        let context = context.ok_or(EffectPrepareError::InvalidSourceBounds)?;
        let dependencies = definition.program().schema().dependencies;
        let rows = match dependencies.coordinates {
            ShaderCoordinates::LocalNormalized => context
                .local_from_capture
                .ok_or(EffectPrepareError::InvalidSourceBounds)?,
            ShaderCoordinates::Scene => context.scene_from_capture,
            ShaderCoordinates::OutputPixels => [
                [1.0, 0.0, context.output_origin[0]],
                [0.0, 1.0, context.output_origin[1]],
            ],
        };
        let position = rows.map(|r| [r[0] as f32, r[1] as f32, r[2] as f32, 0.0]);
        let tolerance = match dependencies.coordinates {
            ShaderCoordinates::LocalNormalized | ShaderCoordinates::Scene => 1.0 / 65536.0,
            ShaderCoordinates::OutputPixels => 1.0 / 256.0,
        };
        // Bound coefficient lowering and arithmetic over the whole capture.
        // Include pixel-center lowering and the generic dot product's seven
        // possible roundings; output coordinates use one addition. Fused
        // evaluation can only reduce this bound.
        if extent.contains(&0) {
            return Err(EffectPrepareError::InvalidSourceBounds);
        }
        let maximum = extent.map(|v| f64::from(v) - 0.5);
        let roundings = if dependencies.coordinates == ShaderCoordinates::OutputPixels {
            0.5
        } else {
            4.0
        };
        for (row, encoded) in rows.iter().zip(position) {
            let lowered = encoded.map(f64::from);
            let coefficient_error = (row[0] - lowered[0]).abs() * maximum[0]
                + (row[1] - lowered[1]).abs() * maximum[1]
                + (row[2] - lowered[2]).abs();
            let magnitude =
                lowered[0].abs() * maximum[0] + lowered[1].abs() * maximum[1] + lowered[2].abs();
            let error = coefficient_error
                + f64::from(f32::EPSILON) * roundings * magnitude
                + f64::from(f32::MIN_POSITIVE) * (maximum[0] + maximum[1] + 1.0);
            if !error.is_finite() || error > tolerance {
                return Err(EffectPrepareError::CaptureCoordinatesOutOfRange);
            }
        }
        let time = if dependencies.authored_time {
            context.authored_time
        } else {
            0.0
        };
        if !time.is_finite() || !(time as f32).is_finite() || time < 0.0 {
            return Err(EffectPrepareError::CaptureCoordinatesOutOfRange);
        }
        let mut values = [[0; 4]; 16];
        for (slot, value) in values.iter_mut().zip(definition.values()) {
            *slot = match *value {
                ShaderValue::Scalar(v) => [(v as f32).to_bits(), 0, 0, 0],
                ShaderValue::Vector2(v) => [(v[0] as f32).to_bits(), (v[1] as f32).to_bits(), 0, 0],
                ShaderValue::Color(c) => [
                    c.red.to_bits(),
                    c.green.to_bits(),
                    c.blue.to_bits(),
                    c.alpha.to_bits(),
                ],
                ShaderValue::Boolean(v) => [u32::from(v), 0, 0, 0],
                ShaderValue::Unsigned(v) => [v, 0, 0, 0],
            };
        }
        Ok(Self {
            position,
            control: [scope_opacity, time as f32, 0.0, 0.0],
            integers: [definition.seed(), 0, 0, 0],
            parameters: values,
        })
    }
}

impl SampleFilter {
    /// Prepare trusted WGSL on this device before installing a new publication.
    /// Errors leave both the cache and existing scopes/output unchanged. Keeping
    /// the returned lease alive makes the program available to prepare_encode.
    pub async fn prepare_shader_program(
        &mut self,
        device: &wgpu::Device,
        source: &WgslEffectProgram,
    ) -> Result<(GpuLease<PreparedShaderProgram>, EffectRasterStats), wgpu::Error> {
        if let Ok(program) = self.shader_program(device, source) {
            return Ok((program, EffectRasterStats::default()));
        }
        let oom = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader_source = wrapper(source);
        #[cfg(test)]
        let shader_source = match self.test_shader_source {
            Some(mutate) => mutate(shader_source),
            None => shader_source,
        };
        let program =
            Program::from_source(device, &shader_source, size_of::<ShaderUniform>() as u64);
        // Pop every guard synchronously before awaiting, so an unrelated caller's
        // device operations cannot become part of this preparation's scope.
        let validation = validation.pop();
        let internal = internal.pop();
        let oom = oom.pop();
        let errors = [validation.await, internal.await, oom.await];
        if let Some(error) = errors.into_iter().flatten().next() {
            return Err(error);
        }
        let program = GpuLease::new(PreparedShaderProgram {
            program,
            source: source.clone(),
            device: device.clone(),
        });
        self.shader_programs
            .retain(|entry| entry.strong_count() > 0);
        self.shader_programs.push(GpuLease::downgrade(&program));
        Ok((
            program,
            EffectRasterStats {
                pipeline_compiles: 1,
                ..Default::default()
            },
        ))
    }

    pub(in crate::gpu) fn remember_shader_program(
        &mut self,
        program: &GpuLease<PreparedShaderProgram>,
    ) {
        self.shader_programs
            .retain(|entry| entry.strong_count() > 0);
        let key = GpuLease::downgrade(program);
        if !self.shader_programs.iter().any(|entry| entry.ptr_eq(&key)) {
            self.shader_programs.push(key);
        }
    }

    pub(in crate::gpu) fn copy_shader_program_cache_from(
        &mut self,
        device: &wgpu::Device,
        previous: &Self,
    ) {
        self.shader_programs = previous
            .shader_programs
            .iter()
            .filter(|entry| {
                entry
                    .upgrade()
                    .is_some_and(|program| program.device == *device)
            })
            .cloned()
            .collect();
    }

    pub(in crate::gpu) fn live_shader_programs(&self) -> usize {
        self.shader_programs
            .iter()
            .filter(|entry| entry.strong_count() > 0)
            .count()
    }

    pub(in crate::gpu) fn shader_program(
        &self,
        device: &wgpu::Device,
        source: &WgslEffectProgram,
    ) -> Result<GpuLease<PreparedShaderProgram>, EffectPrepareError> {
        self.shader_programs
            .iter()
            .filter_map(WeakGpuLease::upgrade)
            .find(|p| p.device == *device && p.source.module() == source.module())
            .ok_or(EffectPrepareError::ShaderProgramNotPrepared)
    }

    pub(in crate::gpu) fn shader_program_for_scope(
        &self,
        device: &wgpu::Device,
        source: &WgslEffectProgram,
        scope: Option<&SampleScope>,
    ) -> Result<GpuLease<PreparedShaderProgram>, EffectPrepareError> {
        let current = scope
            .and_then(|scope| scope.shader.as_ref())
            .filter(|state| {
                state.program.device == *device && state.program.source.module() == source.module()
            })
            .map(|state| GpuLease::clone(&state.program));
        current.map_or_else(|| self.shader_program(device, source), Ok)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_shader_encode(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scope: &mut SampleScope,
        capture: GaussianBlurCapture<'_>,
        parameters: SampleParameters<'_>,
    ) -> Result<EffectRasterStats, EffectPrepareError> {
        let definition = parameters.definition.as_shader().expect("shader pass");
        let uniform = ShaderUniform::new(
            definition,
            [capture.source.width(), capture.source.height()],
            parameters.shader,
            parameters.scope_opacity,
        )?;
        super::validate_texture(capture, &parameters)?;
        let program = self.shader_program_for_scope(device, definition.program(), Some(scope))?;
        let program_changed = scope
            .shader
            .as_ref()
            .is_none_or(|state| !GpuLease::ptr_eq(&state.program, &program));
        let uniform_changed = scope
            .shader
            .as_ref()
            .is_none_or(|state| state.uniform != uniform);
        let result = scope.encode_sample(
            device,
            queue,
            encoder,
            capture,
            &program.program,
            bytemuck::bytes_of(&uniform),
            uniform_changed,
            program_changed,
        );
        if let Some(state) = &mut scope.shader {
            state.program = program;
            state.uniform = uniform;
        } else {
            scope.shader = Some(Box::new(ShaderScope { program, uniform }));
        }
        scope.uniform = None;
        Ok(result)
    }
}

fn wrapper(program: &WgslEffectProgram) -> String {
    use std::fmt::Write;
    let mut source = program.module().to_string();
    source.push_str(WRAPPER);
    source.push_str("let p = NoonShaderParameters(");
    for (index, field) in program.schema().parameters.iter().enumerate() {
        if index > 0 {
            source.push(',');
        }
        let value = match field.kind {
            ShaderParameterType::Scalar { .. } => {
                format!("bitcast<f32>(noon_uniform.parameters[{index}].x)")
            }
            ShaderParameterType::Vector2 { .. } => {
                format!("bitcast<vec2<f32>>(noon_uniform.parameters[{index}].xy)")
            }
            ShaderParameterType::Color => {
                format!("bitcast<vec4<f32>>(noon_uniform.parameters[{index}])")
            }
            ShaderParameterType::Boolean => format!("(noon_uniform.parameters[{index}].x != 0u)"),
            ShaderParameterType::Unsigned { .. } => format!("noon_uniform.parameters[{index}].x"),
        };
        source.push_str(&value);
    }
    if program.schema().parameters.is_empty() {
        source.push_str("0u");
    }
    source.push_str(
        ");\nlet input = NoonShaderInput(vec4<f32>(original.rgb / original.a, 1.0), mapped",
    );
    if program.schema().dependencies.authored_time {
        source.push_str(", noon_uniform.control.y");
    }
    if program.schema().dependencies.seed {
        source.push_str(", noon_uniform.integers.x");
    }
    writeln!(source, ");\nlet value = shade(input, p);\nlet rgba = vec4<f32>(noon_unit(value.x),noon_unit(value.y),noon_unit(value.z),noon_unit(value.w));\nlet alpha = original.a * rgba.a * noon_uniform.control.x;\nreturn vec4<f32>(rgba.rgb * alpha, alpha);\n}}").expect("write String");
    source
}

const WRAPPER: &str = "
struct NoonGpuUniform {
    position_x: vec4<f32>, position_y: vec4<f32>, control: vec4<f32>,
    integers: vec4<u32>, parameters: array<vec4<u32>, 16>,
}
@group(0) @binding(0) var<uniform> noon_uniform: NoonGpuUniform;
@group(0) @binding(1) var noon_source: texture_2d<f32>;
@vertex fn vs_fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let points = array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));
    return vec4<f32>(points[index],0.0,1.0);
}
fn noon_unit(value: f32) -> f32 { return select(0.0, clamp(value,0.0,1.0), value == value); }
@fragment fn fs_sample(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let original = textureLoad(noon_source,vec2<i32>(position.xy),0);
    if original.a <= 0.0 { return vec4<f32>(0.0); }
    let xy = vec3<f32>(position.xy, 1.0);
    let mapped = vec2<f32>(dot(noon_uniform.position_x.xyz,xy),dot(noon_uniform.position_y.xyz,xy));
";
