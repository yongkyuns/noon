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
    @location(6) object_scale: vec2<f32>,
    @location(7) line_stroke_enabled: f32,
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

fn fill_is_enabled(flags: vec2<u32>) -> bool {
    return (flags.x & 1u) != 0u;
}

fn stroke_is_enabled(flags: vec2<u32>) -> bool {
    return (flags.y & 1u) != 0u;
}

fn stroke_is_screen_space(flags: vec2<u32>) -> bool {
    return (flags.y & 2u) != 0u;
}

fn stroke_cap_mode(flags: vec2<f32>) -> u32 {
    return (u32(flags.y) >> 2u) & 3u;
}

fn safe_abs_scale(scale: vec2<f32>) -> vec2<f32> {
    return max(abs(scale), vec2<f32>(0.000001));
}

fn local_stroke_width_for_normal(
    authored_width: f32,
    normal: vec2<f32>,
    scale: vec2<f32>,
    screen_space: bool,
) -> f32 {
    let local_width = max(authored_width, 0.0);
    let normalized = normalize(select(vec2<f32>(1.0, 0.0), normal, length(normal) > 0.000001));
    let inverse_scaled_normal = normalized / safe_abs_scale(scale);
    return select(local_width, local_width * length(inverse_scaled_normal), screen_space);
}

fn world_units_per_pixel(direction: vec2<f32>) -> f32 {
    let clip_vector = direction * camera.clip_scale;
    let pixel_vector = clip_vector * camera.viewport_size * 0.5;
    return 1.0 / max(length(pixel_vector), 0.000001);
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
    output.object_scale = input.scale;
    output.line_stroke_enabled = 0.0;
    return output;
}

@vertex
fn vs_circle(input: VertexInput) -> VertexOutput {
    let radius = max(abs(input.geometry.x), 0.000001);
    let padding = local_axis_padding(input.scale, input.rotation);
    let reveal = clamp(input.geometry.y, 0.0, 1.0);
    let fill_enabled = fill_is_enabled(input.flags);
    let stroke_enabled = stroke_is_enabled(input.flags);
    let derive_creation_stroke = reveal < 1.0 && fill_enabled && !stroke_enabled;
    let has_outline = stroke_enabled || derive_creation_stroke;
    let half_width = max(input.metrics.x, 0.0) * 0.5;
    let scaled_padding = vec2<f32>(half_width);
    let invariant_padding = vec2<f32>(half_width) / safe_abs_scale(input.scale);
    let stroke_padding = select(
        scaled_padding,
        invariant_padding,
        stroke_is_screen_space(input.flags),
    );
    let outline_padding = select(vec2<f32>(0.0), stroke_padding, has_outline);
    let local = input.unit * (vec2<f32>(radius) + outline_padding + padding);
    return make_output(input, local);
}

@vertex
fn vs_rectangle(input: VertexInput) -> VertexOutput {
    let half_size = abs(input.geometry) * 0.5;
    let padding = local_axis_padding(input.scale, input.rotation);
    let half_width = max(input.metrics.x, 0.0) * 0.5;
    let scaled_padding = vec2<f32>(half_width);
    let invariant_padding = vec2<f32>(half_width) / safe_abs_scale(input.scale);
    let stroke_padding = select(
        scaled_padding,
        invariant_padding,
        stroke_is_screen_space(input.flags),
    );
    let outline_padding = select(
        vec2<f32>(0.0),
        stroke_padding,
        stroke_is_enabled(input.flags),
    );
    let local = input.unit * (half_size + outline_padding + padding);
    return make_output(input, local);
}

@vertex
fn vs_line(input: LineVertexInput) -> VertexOutput {
    let reveal = clamp(input.reveal, 0.0, 1.0);
    let revealed_end = mix(input.start, input.end, reveal);
    let screen_space = stroke_is_screen_space(input.flags);
    let authored_width = select(0.0, max(input.metrics.x, 0.0), reveal > 0.0);

    var output: VertexOutput;
    if screen_space {
        let world_start = transform_point(input.start, input.translation, input.scale, input.rotation);
        let world_end = transform_point(revealed_end, input.translation, input.scale, input.rotation);
        let delta = world_end - world_start;
        let segment_length = length(delta);
        var tangent = vec2<f32>(1.0, 0.0);
        if segment_length > 0.000001 {
            tangent = delta / segment_length;
        }
        let normal = vec2<f32>(-tangent.y, tangent.x);
        let half_width = authored_width * 0.5;
        let proxy_half_size = vec2<f32>(
            segment_length * 0.5 + half_width + world_units_per_pixel(tangent),
            half_width + world_units_per_pixel(normal),
        );
        let shape_position = input.unit * proxy_half_size;
        let center = (world_start + world_end) * 0.5;
        let world = center + tangent * shape_position.x + normal * shape_position.y;
        output.position = vec4<f32>((world - camera.center) * camera.clip_scale, 0.0, 1.0);
        output.local = shape_position;
        output.geometry = vec2<f32>(segment_length, authored_width);
        output.object_scale = vec2<f32>(1.0);
    } else {
        let delta = revealed_end - input.start;
        let segment_length = length(delta);
        var tangent = vec2<f32>(1.0, 0.0);
        if segment_length > 0.000001 {
            tangent = delta / segment_length;
        }
        let normal = vec2<f32>(-tangent.y, tangent.x);
        let half_width = authored_width * 0.5;
        let tangent_padding = local_units_per_pixel(tangent, input.scale, input.rotation);
        let normal_padding = local_units_per_pixel(normal, input.scale, input.rotation);
        let proxy_half_size = vec2<f32>(
            segment_length * 0.5 + half_width + tangent_padding,
            half_width + normal_padding,
        );
        let shape_position = input.unit * proxy_half_size;
        let center = (input.start + revealed_end) * 0.5;
        let local = center + tangent * shape_position.x + normal * shape_position.y;
        let world = transform_point(local, input.translation, input.scale, input.rotation);
        output.position = vec4<f32>((world - camera.center) * camera.clip_scale, 0.0, 1.0);
        output.local = shape_position;
        output.geometry = vec2<f32>(segment_length, authored_width);
        output.object_scale = input.scale;
    }
    output.fill = input.fill;
    output.stroke = input.stroke;
    output.metrics = input.metrics;
    output.flags = vec2<f32>(f32(input.flags.x), f32(input.flags.y));
    output.line_stroke_enabled = select(0.0, 1.0, stroke_is_enabled(input.flags));
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

fn cairo_source_color(color: vec4<f32>, opacity: f32) -> vec4<f32> {
    // Cairo stores solid-pattern channels through a rounded 16-bit intermediate,
    // then takes the high byte of the premultiplied result. Quantize the source
    // before fixed-function source-over so WebGPU/WebGL match Cairo's ARGB32
    // compositing instead of applying the render target's nearest-UNORM rounding
    // directly to full-precision float colors.
    let alpha = clamp(color.a * opacity, 0.0, 1.0);
    let rgb16 = floor(clamp(color.rgb * alpha, vec3<f32>(0.0), vec3<f32>(1.0)) * 65535.0 + vec3<f32>(0.5));
    let alpha16 = floor(alpha * 65535.0 + 0.5);
    let rgb8 = floor(rgb16 / 256.0);
    let alpha8 = floor(alpha16 / 256.0);
    return vec4<f32>(rgb8 / 255.0, alpha8 / 255.0);
}

fn covered_color(color: vec4<f32>, opacity: f32, coverage: f32) -> vec4<f32> {
    // Raster coverage is a separate Cairo mask; keep it outside the solid-source
    // quantization so antialiasing continues to operate at fragment precision.
    return cairo_source_color(color, opacity) * coverage;
}

fn source_over(source: vec4<f32>, destination: vec4<f32>) -> vec4<f32> {
    return source + destination * (1.0 - source.a);
}

fn styled_shape_color_with_fill_coverage(
    fill: vec4<f32>,
    stroke: vec4<f32>,
    opacity: f32,
    fill_enabled: bool,
    stroke_enabled: bool,
    signed_distance: f32,
    stroke_width: f32,
    fill_coverage: f32,
) -> vec4<f32> {
    // Match Cairo/VMobject semantics: fill the semantic contour first, then
    // source-over a centered stroke onto the preserved path. Mixing the two
    // premultiplied colors is not equivalent when either layer is translucent.
    let half_stroke_width = max(stroke_width, 0.0) * 0.5;
    let outer_coverage = inside_coverage(signed_distance - half_stroke_width);
    let inner_stroke_coverage = outside_coverage(signed_distance + half_stroke_width);
    let stroke_band_coverage = outer_coverage * inner_stroke_coverage;
    let has_stroke = stroke_enabled && stroke_width > 0.0 && stroke.a > 0.0;
    let fill_layer = select(
        vec4<f32>(0.0),
        covered_color(fill, opacity, fill_coverage),
        fill_enabled,
    );
    if has_stroke {
        let stroke_layer = covered_color(stroke, opacity, stroke_band_coverage);
        return source_over(stroke_layer, fill_layer);
    }
    return fill_layer;
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
    return styled_shape_color_with_fill_coverage(
        fill,
        stroke,
        opacity,
        fill_enabled,
        stroke_enabled,
        signed_distance,
        stroke_width,
        inside_coverage(signed_distance),
    );
}

fn styled_line_color(input: VertexOutput, signed_distance: f32) -> vec4<f32> {
    let coverage = inside_coverage(signed_distance);
    if input.line_stroke_enabled >= 0.5 {
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

fn rectangle_fill_coverage(position: vec2<f32>, half_size: vec2<f32>) -> f32 {
    // A nearest-edge SDF is a good edge AA approximation while opposite sides are
    // separated by at least a pixel. Once a rectangle becomes subpixel, however,
    // both sides of an axis occupy the same filter footprint and the SDF overcounts
    // the covered area. Cairo rasterizes the box area instead. Independent axis masks
    // preserve the existing large-rectangle edge profile and naturally collapse a
    // tiny box toward its projected area without a geometry-size cutoff.
    let x_coverage = inside_coverage(abs(position.x) - half_size.x);
    let y_coverage = inside_coverage(abs(position.y) - half_size.y);
    return x_coverage * y_coverage;
}

fn rectangle_local_normal(position: vec2<f32>, half_size: vec2<f32>) -> vec2<f32> {
    let offset = abs(position) - half_size;
    if offset.x > 0.0 && offset.y > 0.0 {
        return normalize(vec2<f32>(sign(position.x) * offset.x, sign(position.y) * offset.y));
    }
    if offset.x > offset.y {
        return vec2<f32>(sign(position.x), 0.0);
    }
    return vec2<f32>(0.0, sign(position.y));
}

fn capsule_signed_distance(position: vec2<f32>, half_length: f32, radius: f32) -> f32 {
    let offset = vec2<f32>(max(abs(position.x) - half_length, 0.0), position.y);
    return length(offset) - radius;
}

@fragment
fn fs_circle(input: VertexOutput) -> @location(0) vec4<f32> {
    let radius = max(abs(input.geometry.x), 0.000001);
    let reveal = clamp(input.geometry.y, 0.0, 1.0);
    let signed_distance = length(input.local) - radius;
    let screen_space_stroke = (u32(input.flags.y) & 2u) != 0u;
    let local_normal = normalize(select(vec2<f32>(1.0, 0.0), input.local, length(input.local) > 0.000001));
    let stroke_width = local_stroke_width_for_normal(
        input.metrics.x,
        local_normal,
        input.object_scale,
        screen_space_stroke,
    );
    let fill_enabled = input.flags.x > 0.5;
    let stroke_enabled = (u32(input.flags.y) & 1u) != 0u;

    // All derivative-dependent coverage is evaluated before reveal-dependent
    // control flow. `reveal` is interpolated, so WebGPU requires derivatives to
    // stay in uniform control flow even though each instance supplies one value.
    let final_color = styled_shape_color(
        input.fill,
        input.stroke,
        input.metrics.y,
        fill_enabled,
        stroke_enabled,
        signed_distance,
        stroke_width,
    );
    let tau = 6.283185307179586;
    var angle = atan2(input.local.y, input.local.x);
    if angle < 0.0 {
        angle += tau;
    }
    let progress = angle / tau;
    let progress_edge = max(fwidth(progress), 0.00001);
    let half_stroke_width = stroke_width * 0.5;
    let outer_coverage = inside_coverage(signed_distance - half_stroke_width);
    let inner_coverage = outside_coverage(signed_distance + half_stroke_width);
    let ring_coverage = outer_coverage * inner_coverage;
    let fill_coverage = inside_coverage(signed_distance);
    let head_angle = reveal * tau;
    let head_center = radius * vec2<f32>(cos(head_angle), sin(head_angle));
    let start_center = vec2<f32>(radius, 0.0);
    let local_head_distance = length(input.local - head_center) - half_stroke_width;
    let local_start_distance = length(input.local - start_center) - half_stroke_width;
    let world_head_distance = length((input.local - head_center) * input.object_scale)
        - max(input.metrics.x, 0.0) * 0.5;
    let world_start_distance = length((input.local - start_center) * input.object_scale)
        - max(input.metrics.x, 0.0) * 0.5;
    let head_cap = inside_coverage(select(local_head_distance, world_head_distance, screen_space_stroke));
    let start_cap = inside_coverage(select(local_start_distance, world_start_distance, screen_space_stroke));

    if reveal >= 1.0 {
        return final_color;
    }
    if reveal <= 0.0 {
        return vec4<f32>(0.0);
    }

    // Circle Create remains entirely analytic. The SDF gives an exact circle;
    // angular progress reveals its outline and an analytic disk supplies the
    // moving round head, so there is no faceted temporary mesh or endpoint pop.
    let body_reveal = 1.0 - smoothstep(reveal, reveal + progress_edge, progress);
    let has_creation_stroke = stroke_width > 0.0 && (stroke_enabled || fill_enabled);
    let stroke_coverage = select(
        0.0,
        max(ring_coverage * body_reveal, max(head_cap, start_cap)),
        has_creation_stroke,
    );

    let fill_alpha = smoothstep(0.0, 1.0, reveal);
    let fill_layer = select(
        vec4<f32>(0.0),
        covered_color(input.fill, input.metrics.y * fill_alpha, fill_coverage),
        fill_enabled,
    );
    let derive_creation_stroke = fill_enabled && !stroke_enabled;
    let creation_outline_alpha = select(
        1.0,
        1.0 - smoothstep(0.75, 1.0, reveal),
        derive_creation_stroke,
    );
    let stroke_color = select(input.stroke, input.fill, derive_creation_stroke);
    let stroke_layer = covered_color(
        stroke_color,
        input.metrics.y * creation_outline_alpha,
        stroke_coverage,
    );
    return stroke_layer + fill_layer * (1.0 - stroke_layer.a);
}

@fragment
fn fs_rectangle(input: VertexOutput) -> @location(0) vec4<f32> {
    let half_size = max(abs(input.geometry) * 0.5, vec2<f32>(0.000001));
    let signed_distance = rectangle_signed_distance(input.local, half_size);
    let fill_coverage = rectangle_fill_coverage(input.local, half_size);
    let stroke_flags = u32(input.flags.y);
    let stroke_width = local_stroke_width_for_normal(
        input.metrics.x,
        rectangle_local_normal(input.local, half_size),
        input.object_scale,
        (stroke_flags & 2u) != 0u,
    );
    return styled_shape_color_with_fill_coverage(
        input.fill,
        input.stroke,
        input.metrics.y,
        input.flags.x > 0.5,
        (stroke_flags & 1u) != 0u,
        signed_distance,
        stroke_width,
        fill_coverage,
    );
}

@fragment
fn fs_line(input: VertexOutput) -> @location(0) vec4<f32> {
    let half_length = input.geometry.x * 0.5;
    let radius = input.geometry.y * 0.5;
    let cap_mode = stroke_cap_mode(input.flags);
    var signed_distance = capsule_signed_distance(input.local, half_length, radius);
    if cap_mode == 1u {
        // Cairo/Manim BUTT caps terminate at the semantic endpoints. The proxy
        // quad is intentionally larger for antialiasing; the SDF clips coverage.
        signed_distance = rectangle_signed_distance(
            input.local,
            vec2<f32>(half_length, radius),
        );
    } else if cap_mode == 2u {
        // SQUARE extends by exactly one half stroke width along the tangent.
        signed_distance = rectangle_signed_distance(
            input.local,
            vec2<f32>(half_length + radius, radius),
        );
    }
    let visible = select(0.0, 1.0, input.geometry.y > 0.0);
    return styled_line_color(input, signed_distance) * visible;
}
