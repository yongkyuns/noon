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
    @location(1) surface: u32,
    @location(2) translation: vec2<f32>,
    @location(3) scale: vec2<f32>,
    @location(4) rotation: f32,
    @location(5) fill: vec4<f32>,
    @location(6) stroke: vec4<f32>,
    @location(7) metrics: vec2<f32>,
    @location(8) flags: vec2<u32>,
};

struct PathVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

fn premultiplied(color: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(color.rgb * color.a, color.a);
}

@vertex
fn vs_path(input: PathVertexInput) -> PathVertexOutput {
    let c = cos(input.rotation);
    let s = sin(input.rotation);
    let scaled = input.local * input.scale;
    let world = vec2<f32>(
        c * scaled.x - s * scaled.y,
        s * scaled.x + c * scaled.y,
    ) + input.translation;

    var output: PathVertexOutput;
    output.position = vec4<f32>((world - camera.center) * camera.clip_scale, 0.0, 1.0);
    let is_stroke = input.surface == 1u;
    let enabled = select(input.flags.x != 0u, input.flags.y != 0u, is_stroke);
    let color = select(input.fill, input.stroke, is_stroke);
    output.color = select(vec4<f32>(0.0), premultiplied(color) * input.metrics.y, enabled);
    return output;
}

@fragment
fn fs_path(input: PathVertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
