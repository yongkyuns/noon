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
    @location(10) path_params: vec2<f32>,
};

struct PathVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_path(input: PathVertexInput) -> PathVertexOutput {
    var output: PathVertexOutput;
    output.position = vec4<f32>((input.local - camera.center) * camera.clip_scale, 0.0, 1.0);
    output.color = vec4<f32>(1.0, 0.0, 1.0, 1.0);
    return output;
}

@fragment
fn fs_path(input: PathVertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
