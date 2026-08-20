struct Camera {
    center: vec2<f32>,
    clip_scale: vec2<f32>,
    viewport_size: vec2<f32>,
    padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

struct PathVertexInput {
    @location(0) local: vec2<f32>,
    @location(1) target_local: vec2<f32>,
    @location(2) surface_and_progress: u32,
    @location(3) translation: vec2<f32>,
    @location(4) scale: vec2<f32>,
    @location(5) rotation: f32,
    @location(6) fill: vec4<f32>,
    @location(7) stroke: vec4<f32>,
    @location(8) metrics: vec2<f32>,
    @location(9) flags: vec2<u32>,
};

struct PathVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) path_progress: f32,
    @location(2) reveal: f32,
};

fn premultiplied(color: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(color.rgb * color.a, color.a);
}

@vertex
fn vs_path(input: PathVertexInput) -> PathVertexOutput {
    let is_stroke = (input.surface_and_progress & 1u) == 1u;
    let is_morph = (input.surface_and_progress & 33554432u) != 0u;
    let encoded_progress = (input.surface_and_progress >> 1u) & 16777215u;
    let path_progress = f32(encoded_progress) / 16777215.0;
    let scalar = clamp(input.metrics.x, 0.0, 1.0);
    let local = select(input.local, mix(input.local, input.target_local, scalar), is_morph);

    let c = cos(input.rotation);
    let s = sin(input.rotation);
    let scaled = local * input.scale;
    let world = vec2<f32>(
        c * scaled.x - s * scaled.y,
        s * scaled.x + c * scaled.y,
    ) + input.translation;

    var output: PathVertexOutput;
    output.position = vec4<f32>((world - camera.center) * camera.clip_scale, 0.0, 1.0);
    let enabled = select(input.flags.x != 0u, input.flags.y != 0u, is_stroke);
    let color = select(input.fill, input.stroke, is_stroke);
    output.color = select(vec4<f32>(0.0), premultiplied(color) * input.metrics.y, enabled);
    output.path_progress = path_progress;
    output.reveal = select(scalar, 1.0, is_morph);
    return output;
}

@fragment
fn fs_path(input: PathVertexOutput) -> @location(0) vec4<f32> {
    if input.reveal <= 0.0 {
        return vec4<f32>(0.0);
    }
    if input.reveal >= 1.0 {
        return input.color;
    }

    let edge = max(fwidth(input.path_progress), 0.00001);
    let coverage = 1.0 - smoothstep(input.reveal, input.reveal + edge, input.path_progress);
    return input.color * coverage;
}
