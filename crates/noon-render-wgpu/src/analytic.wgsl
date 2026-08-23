struct Camera {
    center: vec2<f32>,
    clip_scale: vec2<f32>,
    viewport_size: vec2<f32>,
    padding: vec2<f32>,
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
    @location(10) reveal: f32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) geometry: vec2<f32>,
    @location(2) fill: vec4<f32>,
    @location(3) stroke: vec4<f32>,
    @location(4) metrics: vec2<f32>,
    @location(5) flags: vec2<f32>,
};

fn rotate_vector(local: vec2<f32>, rotation: f32) -> vec2<f32> {
    let c = cos(rotation);
    let s = sin(rotation);
    return vec2<f32>(
        c * local.x - s * local.y,
        s * local.x + c * local.y,
    );
}

fn transform_point(
    local: vec2<f32>,
    translation: vec2<f32>,
    scale: vec2<f32>,
    rotation: f32,
) -> vec2<f32> {
    let rotated = rotate_vector(local * scale, rotation);
    return rotated + translation;
}

fn local_units_per_pixel(
    direction: vec2<f32>,
    scale: vec2<f32>,
    rotation: f32,
) -> f32 {
    let world_vector = rotate_vector(direction * scale, rotation);
    let clip_vector = world_vector * camera.clip_scale;
    let pixel_vector = clip_vector * camera.viewport_size * 0.5;
    return 1.0 / max(length(pixel_vector), 0.000001);
}

fn local_axis_padding(scale: vec2<f32>, rotation: f32) -> vec2<f32> {
    return vec2<f32>(
        local_units_per_pixel(vec2<f32>(1.0, 0.0), scale, rotation),
        local_units_per_pixel(vec2<f32>(0.0, 1.0), scale, rotation),
    );
}

fn stroke_half_width(metrics: vec2<f32>, flags: vec2<u32>) -> f32 {
    return select(0.0, max(metrics.x, 0.0) * 0.5, flags.y != 0u);
}

fn make_output(input: VertexInput, local: vec2<f32>) -> VertexOutput {
    var output: VertexOutput;
    let world = transform_point(local, input.translation, input.scale, input.rotation);
    let clip = (world - camera.center) * camera.clip_scale;
    output.position = vec4<f32>(clip, 0.0, 1.0);
    output.local = local;
    output.geometry = input.geometry;
    output.fill = input.fill;
    output.stroke = input.stroke;
    output.metrics = input.metrics;
    output.flags = vec2<f32>(f32(input.flags.x), f32(input.flags.y));
    return output;
}

@vertex
fn vs_circle(input: VertexInput) -> VertexOutput {
    let radius = max(abs(input.geometry.x), 0.000001);
    let padding = local_axis_padding(input.scale, input.rotation);
    let stroke_padding = stroke_half_width(input.metrics, input.flags);
    let local = input.unit * (vec2<f32>(radius + stroke_padding) + padding);
    return make_output(input, local);
}

@vertex
fn vs_rectangle(input: VertexInput) -> VertexOutput {
    let half_size = abs(input.geometry) * 0.5;
    let padding = local_axis_padding(input.scale, input.rotation);
    let stroke_padding = stroke_half_width(input.metrics, input.flags);
    let local = input.unit * (half_size + vec2<f32>(stroke_padding) + padding);
    return make_output(input, local);
}

@vertex
fn vs_line(input: LineVertexInput) -> VertexOutput {
    let reveal = clamp(input.reveal, 0.0, 1.0);
    let revealed_end = mix(input.start, input.end, reveal);
    let delta = revealed_end - input.start;
    let segment_length = length(delta);
    var tangent = vec2<f32>(1.0, 0.0);
    if segment_length > 0.000001 {
        tangent = delta / segment_length;
    }
    let normal = vec2<f32>(-tangent.y, tangent.x);
    let width = select(0.0, max(input.metrics.x, 0.0), reveal > 0.0);
    let half_width = width * 0.5;
    let tangent_padding = local_units_per_pixel(tangent, input.scale, input.rotation);
    let normal_padding = local_units_per_pixel(normal, input.scale, input.rotation);
    let proxy_half_size = vec2<f32>(
        segment_length * 0.5 + half_width + tangent_padding,
        half_width + normal_padding,
    );
    let shape_position = input.unit * proxy_half_size;
    let center = (input.start + revealed_end) * 0.5;
    let local = center + tangent * shape_position.x + normal * shape_position.y;

    var output: VertexOutput;
    let world = transform_point(local, input.translation, input.scale, input.rotation);
    let clip = (world - camera.center) * camera.clip_scale;
    output.position = vec4<f32>(clip, 0.0, 1.0);
    output.local = shape_position;
    output.geometry = vec2<f32>(segment_length, width);
    output.fill = input.fill;
    output.stroke = input.stroke;
    output.metrics = input.metrics;
    output.flags = vec2<f32>(f32(input.flags.x), f32(input.flags.y));
    return output;
}

fn transition_width(signed_distance: f32) -> f32 {
    return max(fwidth(signed_distance), 0.000001);
}

fn inside_coverage(signed_distance: f32) -> f32 {
    let half_width = transition_width(signed_distance) * 0.5;
    return 1.0 - smoothstep(-half_width, half_width, signed_distance);
}

fn outside_coverage(signed_distance: f32) -> f32 {
    let half_width = transition_width(signed_distance) * 0.5;
    return smoothstep(-half_width, half_width, signed_distance);
}

fn premultiplied(color: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(color.rgb * color.a, color.a);
}

fn covered_color(color: vec4<f32>, opacity: f32, coverage: f32) -> vec4<f32> {
    return premultiplied(color) * (opacity * coverage);
}

fn styled_shape_color(
    fill: vec4<f32>,
    stroke: vec4<f32>,
    opacity: f32,
    fill_enabled: bool,
    stroke_enabled: bool,
    signed_distance: f32,
    stroke_width: f32,
) -> vec4<f32> {
    // Match VectorPath/Lyon semantics: the authored outline is the stroke centerline.
    // A stroke extends half its width outside and half inside the semantic boundary.
    let half_stroke_width = max(stroke_width, 0.0) * 0.5;
    let fill_coverage = inside_coverage(signed_distance);
    let outer_coverage = inside_coverage(signed_distance - half_stroke_width);
    let stroke_coverage = outside_coverage(signed_distance + half_stroke_width);
    let has_stroke = stroke_enabled && stroke_width > 0.0;
    if has_stroke {
        if fill_enabled {
            let color = mix(premultiplied(fill), premultiplied(stroke), stroke_coverage);
            return color * (opacity * outer_coverage);
        }
        return covered_color(stroke, opacity, outer_coverage * stroke_coverage);
    }

    if fill_enabled {
        return covered_color(fill, opacity, fill_coverage);
    }
    return vec4<f32>(0.0);
}

fn styled_line_color(input: VertexOutput, signed_distance: f32) -> vec4<f32> {
    let coverage = inside_coverage(signed_distance);
    if input.flags.y > 0.5 {
        return covered_color(input.stroke, input.metrics.y, coverage);
    }
    if input.flags.x > 0.5 {
        return covered_color(input.fill, input.metrics.y, coverage);
    }
    return vec4<f32>(0.0);
}

fn rectangle_signed_distance(position: vec2<f32>, half_size: vec2<f32>) -> f32 {
    let offset = abs(position) - half_size;
    return length(max(offset, vec2<f32>(0.0))) + min(max(offset.x, offset.y), 0.0);
}

fn capsule_signed_distance(position: vec2<f32>, half_length: f32, radius: f32) -> f32 {
    let offset = vec2<f32>(max(abs(position.x) - half_length, 0.0), position.y);
    return length(offset) - radius;
}

@fragment
fn fs_circle(input: VertexOutput) -> @location(0) vec4<f32> {
    let radius = max(abs(input.geometry.x), 0.000001);
    let signed_distance = length(input.local) - radius;
    let stroke_width = max(input.metrics.x, 0.0);
    return styled_shape_color(
        input.fill,
        input.stroke,
        input.metrics.y,
        input.flags.x > 0.5,
        input.flags.y > 0.5,
        signed_distance,
        stroke_width,
    );
}

@fragment
fn fs_rectangle(input: VertexOutput) -> @location(0) vec4<f32> {
    let half_size = max(abs(input.geometry) * 0.5, vec2<f32>(0.000001));
    let signed_distance = rectangle_signed_distance(input.local, half_size);
    let stroke_width = max(input.metrics.x, 0.0);
    return styled_shape_color(
        input.fill,
        input.stroke,
        input.metrics.y,
        input.flags.x > 0.5,
        input.flags.y > 0.5,
        signed_distance,
        stroke_width,
    );
}

@fragment
fn fs_line(input: VertexOutput) -> @location(0) vec4<f32> {
    let half_length = input.geometry.x * 0.5;
    let radius = input.geometry.y * 0.5;
    let signed_distance = capsule_signed_distance(input.local, half_length, radius);
    let visible = select(0.0, 1.0, input.geometry.y > 0.0);
    return styled_line_color(input, signed_distance) * visible;
}
