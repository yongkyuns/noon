struct Camera {
    center: vec2<f32>,
    clip_scale: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

struct VertexInput {
    @location(0) unit: vec2<f32>,
    @location(1) translation: vec2<f32>,
    @location(2) scale: vec2<f32>,
    @location(3) rotation: f32,
    @location(4) geometry: vec2<f32>,
    @location(5) fill: vec4<f32>,
    @location(6) stroke: vec4<f32>,
    @location(7) metrics: vec2<f32>,
    @location(8) flags: vec2<u32>,
};

struct LineVertexInput {
    @location(0) unit: vec2<f32>,
    @location(1) translation: vec2<f32>,
    @location(2) scale: vec2<f32>,
    @location(3) rotation: f32,
    @location(4) start: vec2<f32>,
    @location(5) fill: vec4<f32>,
    @location(6) stroke: vec4<f32>,
    @location(7) metrics: vec2<f32>,
    @location(8) flags: vec2<u32>,
    @location(9) end: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) unit: vec2<f32>,
    @location(1) geometry: vec2<f32>,
    @location(2) fill: vec4<f32>,
    @location(3) stroke: vec4<f32>,
    @location(4) metrics: vec2<f32>,
    @location(5) flags: vec2<f32>,
};

fn transform_point(
    local: vec2<f32>,
    translation: vec2<f32>,
    scale: vec2<f32>,
    rotation: f32,
) -> vec2<f32> {
    let scaled = local * scale;
    let c = cos(rotation);
    let s = sin(rotation);
    let rotated = vec2<f32>(
        c * scaled.x - s * scaled.y,
        s * scaled.x + c * scaled.y,
    );
    return rotated + translation;
}

fn make_output(input: VertexInput, local: vec2<f32>) -> VertexOutput {
    var output: VertexOutput;
    let world = transform_point(local, input.translation, input.scale, input.rotation);
    let clip = (world - camera.center) * camera.clip_scale;
    output.position = vec4<f32>(clip, 0.0, 1.0);
    output.unit = input.unit;
    output.geometry = input.geometry;
    output.fill = input.fill;
    output.stroke = input.stroke;
    output.metrics = input.metrics;
    output.flags = vec2<f32>(f32(input.flags.x), f32(input.flags.y));
    return output;
}

@vertex
fn vs_circle(input: VertexInput) -> VertexOutput {
    let local = input.unit * input.geometry.x;
    return make_output(input, local);
}

@vertex
fn vs_rectangle(input: VertexInput) -> VertexOutput {
    let local = input.unit * input.geometry * 0.5;
    return make_output(input, local);
}

@vertex
fn vs_line(input: LineVertexInput) -> VertexOutput {
    let delta = input.end - input.start;
    let segment_length = max(length(delta), 0.000001);
    let tangent = delta / segment_length;
    let normal = vec2<f32>(-tangent.y, tangent.x);
    let along = (input.unit.x + 1.0) * 0.5;
    let half_width = max(input.metrics.x * 0.5, 0.000001);
    let local = input.start + delta * along + normal * input.unit.y * half_width;

    var output: VertexOutput;
    let world = transform_point(local, input.translation, input.scale, input.rotation);
    let clip = (world - camera.center) * camera.clip_scale;
    output.position = vec4<f32>(clip, 0.0, 1.0);
    output.unit = input.unit;
    output.geometry = vec2<f32>(segment_length, input.metrics.x);
    output.fill = input.fill;
    output.stroke = input.stroke;
    output.metrics = input.metrics;
    output.flags = vec2<f32>(f32(input.flags.x), f32(input.flags.y));
    return output;
}

fn styled_color(
    fill: vec4<f32>,
    stroke: vec4<f32>,
    opacity: f32,
    fill_enabled: bool,
    stroke_enabled: bool,
    stroke_region: bool,
) -> vec4<f32> {
    var color: vec4<f32>;
    if stroke_region && stroke_enabled {
        color = stroke;
    } else if fill_enabled {
        color = fill;
    } else {
        discard;
    }
    color.a = color.a * opacity;
    return color;
}

@fragment
fn fs_circle(input: VertexOutput) -> @location(0) vec4<f32> {
    let distance = length(input.unit);
    if distance > 1.0 {
        discard;
    }

    let radius = max(abs(input.geometry.x), 0.000001);
    let stroke_fraction = clamp(input.metrics.x / radius, 0.0, 1.0);
    let stroke_region = distance >= 1.0 - stroke_fraction;
    return styled_color(
        input.fill,
        input.stroke,
        input.metrics.y,
        input.flags.x > 0.5,
        input.flags.y > 0.5,
        stroke_region,
    );
}

@fragment
fn fs_rectangle(input: VertexOutput) -> @location(0) vec4<f32> {
    let edge = max(abs(input.unit.x), abs(input.unit.y));
    let min_dimension = max(min(abs(input.geometry.x), abs(input.geometry.y)), 0.000001);
    let stroke_fraction = clamp((2.0 * input.metrics.x) / min_dimension, 0.0, 1.0);
    let stroke_region = edge >= 1.0 - stroke_fraction;
    return styled_color(
        input.fill,
        input.stroke,
        input.metrics.y,
        input.flags.x > 0.5,
        input.flags.y > 0.5,
        stroke_region,
    );
}

@fragment
fn fs_line(input: VertexOutput) -> @location(0) vec4<f32> {
    return styled_color(
        input.fill,
        input.stroke,
        input.metrics.y,
        input.flags.x > 0.5,
        input.flags.y > 0.5,
        true,
    );
}
