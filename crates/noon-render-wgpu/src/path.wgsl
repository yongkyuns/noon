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
    @location(1) surface_and_progress: u32,
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
    @location(1) path_progress: f32,
    @location(2) reveal: f32,
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

    let is_stroke = (input.surface_and_progress & 1u) == 1u;
    let encoded_progress = input.surface_and_progress >> 1u;
    // The CPU uses an exact 24-bit integer domain for normalized path
    // progress so both endpoints survive the f32 conversion exactly.
    let path_progress = f32(encoded_progress) / 16777215.0;

    var output: PathVertexOutput;
    output.position = vec4<f32>((world - camera.center) * camera.clip_scale, 0.0, 1.0);
    let enabled = select(input.flags.x != 0u, input.flags.y != 0u, is_stroke);
    let color = select(input.fill, input.stroke, is_stroke);
    output.color = select(vec4<f32>(0.0), premultiplied(color) * input.metrics.y, enabled);
    output.path_progress = path_progress;
    // Path stroke width is already baked into the mesh. The path instance
    // reuses metrics.x to carry normalized reveal without increasing stride.
    output.reveal = clamp(input.metrics.x, 0.0, 1.0);
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

    // Interpolated arc-length progress clips the already-tessellated stroke;
    // fwidth gives the moving reveal edge a pixel-scale antialiasing ramp.
    let edge = max(fwidth(input.path_progress), 0.00001);
    let coverage = 1.0 - smoothstep(input.reveal, input.reveal + edge, input.path_progress);
    return input.color * coverage;
}
