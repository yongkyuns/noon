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
    @location(1) path_progress: f32,
    @location(2) reveal: f32,
};

fn premultiplied(color: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(color.rgb * color.a, color.a);
}

@vertex
fn vs_path(input: PathVertexInput) -> PathVertexOutput {
    let is_stroke = (input.surface_and_progress & 1u) == 1u;
    let encoded_progress = input.surface_and_progress >> 1u;
    let path_progress = f32(encoded_progress) / 16777215.0;
    let morph = clamp(input.path_params.y, 0.0, 1.0);
    let reveal = clamp(input.path_params.x, 0.0, 1.0);
    let local = mix(input.local, input.target_local, morph);

    let c = cos(input.rotation);
    let s = sin(input.rotation);
    let scaled = local * input.scale;
    let world = vec2<f32>(
        c * scaled.x - s * scaled.y,
        s * scaled.x + c * scaled.y,
    ) + input.translation;

    var output: PathVertexOutput;
    output.position = vec4<f32>((world - camera.center) * camera.clip_scale, 0.0, 1.0);

    // A fill-only path has no visible stroke to reveal. During a partial reveal,
    // derive the creation outline from the fill color while suppressing the fill
    // surface. At reveal == 1 the authored fill/stroke style is restored exactly.
    let derive_creation_stroke = reveal < 1.0 && input.flags.x != 0u && input.flags.y == 0u;
    let authored_enabled = select(input.flags.x != 0u, input.flags.y != 0u, is_stroke);
    let enabled = select(authored_enabled, is_stroke, derive_creation_stroke);
    let authored_color = select(input.fill, input.stroke, is_stroke);
    let color = select(authored_color, input.fill, derive_creation_stroke);
    output.color = select(vec4<f32>(0.0), premultiplied(color) * input.metrics.y, enabled);
    output.path_progress = path_progress;
    output.reveal = reveal;
    return output;
}

@fragment
fn fs_path(input: PathVertexOutput) -> @location(0) vec4<f32> {
    // Fragment derivatives must execute in uniform control flow. `reveal` is an
    // interpolated input, so evaluate fwidth before any branch that depends on it.
    let edge = max(fwidth(input.path_progress), 0.00001);

    if input.reveal <= 0.0 {
        return vec4<f32>(0.0);
    }
    if input.reveal >= 1.0 {
        return input.color;
    }

    let coverage = 1.0 - smoothstep(input.reveal, input.reveal + edge, input.path_progress);
    return input.color * coverage;
}
