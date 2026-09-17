struct Camera {
    center: vec2<f32>,
    clip_scale: vec2<f32>,
    viewport_size: vec2<f32>,
    padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

const TRIANGLE_COVERAGE_FLAG: u32 = 33554432u;
const PATH_PROGRESS_MASK: u32 = 33554431u;

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
    @location(11) triangle_a: vec2<f32>,
    @location(12) triangle_b: vec2<f32>,
    @location(13) triangle_c: vec2<f32>,
};

struct PathVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) path_progress: f32,
    @location(2) reveal: f32,
    @location(3) is_stroke: f32,
    @location(4) @interpolate(flat) triangle_a: vec2<f32>,
    @location(5) @interpolate(flat) triangle_b: vec2<f32>,
    @location(6) @interpolate(flat) triangle_c: vec2<f32>,
    @location(7) @interpolate(flat) exact_triangle: u32,
};

struct CompactPathVertexInput {
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

struct ClippedPolygon {
    points: array<vec2<f32>, 8>,
    count: u32,
};

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

fn transform_path_point(local: vec2<f32>, input: PathVertexInput) -> vec2<f32> {
    let c = cos(input.rotation);
    let s = sin(input.rotation);
    let scaled = local * input.scale;
    return vec2<f32>(
        c * scaled.x - s * scaled.y,
        s * scaled.x + c * scaled.y,
    ) + input.translation;
}

fn world_to_pixel(world: vec2<f32>) -> vec2<f32> {
    let clip = (world - camera.center) * camera.clip_scale;
    return vec2<f32>(
        (clip.x + 1.0) * camera.viewport_size.x * 0.5,
        (1.0 - clip.y) * camera.viewport_size.y * 0.5,
    );
}

fn pixel_to_clip(pixel: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        pixel.x * 2.0 / camera.viewport_size.x - 1.0,
        1.0 - pixel.y * 2.0 / camera.viewport_size.y,
    );
}

@vertex
fn vs_path(input: PathVertexInput) -> PathVertexOutput {
    let is_stroke = (input.surface_and_progress & 1u) == 1u;
    let encoded_progress = (input.surface_and_progress & PATH_PROGRESS_MASK) >> 1u;
    let path_progress = f32(encoded_progress) / 16777215.0;
    let morph = clamp(input.path_params.y, 0.0, 1.0);
    let reveal = clamp(input.path_params.x, 0.0, 1.0);
    let local = mix(input.local, input.target_local, morph);
    let exact_triangle = (input.surface_and_progress & TRIANGLE_COVERAGE_FLAG) != 0u;

    var output: PathVertexOutput;
    if exact_triangle {
        let a = world_to_pixel(transform_path_point(input.triangle_a, input));
        let b = world_to_pixel(transform_path_point(input.triangle_b, input));
        let c = world_to_pixel(transform_path_point(input.triangle_c, input));
        let minimum = min(a, min(b, c)) - vec2<f32>(1.0);
        let maximum = max(a, max(b, c)) + vec2<f32>(1.0);
        let quad_position = (local + vec2<f32>(1.0)) * 0.5;
        let pixel = mix(minimum, maximum, quad_position);
        output.position = vec4<f32>(pixel_to_clip(pixel), 0.0, 1.0);
        output.triangle_a = a;
        output.triangle_b = b;
        output.triangle_c = c;
        output.exact_triangle = 1u;
    } else {
        let world = transform_path_point(local, input);
        output.position = vec4<f32>((world - camera.center) * camera.clip_scale, 0.0, 1.0);
        output.triangle_a = vec2<f32>(0.0);
        output.triangle_b = vec2<f32>(0.0);
        output.triangle_c = vec2<f32>(0.0);
        output.exact_triangle = 0u;
    }

    let fill_enabled = (input.flags.x & 1u) != 0u;
    let stroke_enabled = (input.flags.y & 1u) != 0u;
    let derive_creation_stroke = reveal < 1.0 && fill_enabled && !stroke_enabled;
    let authored_enabled = select(fill_enabled, stroke_enabled, is_stroke);
    let enabled = authored_enabled || (is_stroke && derive_creation_stroke);
    let authored_color = select(input.fill, input.stroke, is_stroke);
    let color = select(authored_color, input.fill, is_stroke && derive_creation_stroke);
    var creation_outline_alpha = 1.0;
    if is_stroke && derive_creation_stroke {
        creation_outline_alpha = 1.0 - smoothstep(0.75, 1.0, reveal);
    }
    output.color = select(
        vec4<f32>(0.0),
        cairo_source_color(color, input.metrics.y * creation_outline_alpha),
        enabled,
    );
    output.path_progress = path_progress;
    output.reveal = reveal;
    output.is_stroke = select(0.0, 1.0, is_stroke);
    return output;
}

@vertex
fn vs_path_compact(input: CompactPathVertexInput) -> PathVertexOutput {
    let is_stroke = (input.surface_and_progress & 1u) == 1u;
    let encoded_progress = (input.surface_and_progress & PATH_PROGRESS_MASK) >> 1u;
    let path_progress = f32(encoded_progress) / 16777215.0;
    let reveal = clamp(input.path_params.x, 0.0, 1.0);
    let local = mix(input.local, input.target_local, clamp(input.path_params.y, 0.0, 1.0));
    let c = cos(input.rotation);
    let s = sin(input.rotation);
    let scaled = local * input.scale;
    let world = vec2<f32>(c * scaled.x - s * scaled.y, s * scaled.x + c * scaled.y) + input.translation;
    let fill_enabled = (input.flags.x & 1u) != 0u;
    let stroke_enabled = (input.flags.y & 1u) != 0u;
    let derive_creation_stroke = reveal < 1.0 && fill_enabled && !stroke_enabled;
    let authored_enabled = select(fill_enabled, stroke_enabled, is_stroke);
    let enabled = authored_enabled || (is_stroke && derive_creation_stroke);
    let authored_color = select(input.fill, input.stroke, is_stroke);
    let color = select(authored_color, input.fill, is_stroke && derive_creation_stroke);
    var creation_outline_alpha = 1.0;
    if is_stroke && derive_creation_stroke {
        creation_outline_alpha = 1.0 - smoothstep(0.75, 1.0, reveal);
    }
    var output: PathVertexOutput;
    output.position = vec4<f32>((world - camera.center) * camera.clip_scale, 0.0, 1.0);
    output.color = select(
        vec4<f32>(0.0),
        cairo_source_color(color, input.metrics.y * creation_outline_alpha),
        enabled,
    );
    output.path_progress = path_progress;
    output.reveal = reveal;
    output.is_stroke = select(0.0, 1.0, is_stroke);
    output.triangle_a = vec2<f32>(0.0);
    output.triangle_b = vec2<f32>(0.0);
    output.triangle_c = vec2<f32>(0.0);
    output.exact_triangle = 0u;
    return output;
}

fn clip_polygon_axis(
    polygon: ClippedPolygon,
    axis: u32,
    boundary: f32,
    keep_greater: bool,
) -> ClippedPolygon {
    var output: ClippedPolygon;
    output.count = 0u;
    if polygon.count == 0u {
        return output;
    }

    var index = 0u;
    loop {
        if index >= polygon.count {
            break;
        }
        let next = select(index + 1u, 0u, index + 1u == polygon.count);
        let p = polygon.points[index];
        let q = polygon.points[next];
        let p_coordinate = select(p.y, p.x, axis == 0u);
        let q_coordinate = select(q.y, q.x, axis == 0u);
        let p_inside = select(p_coordinate <= boundary, p_coordinate >= boundary, keep_greater);
        let q_inside = select(q_coordinate <= boundary, q_coordinate >= boundary, keep_greater);

        // A triangle clipped by four half-planes has at most seven vertices:
        // each clip adds at most one, so the fixed eight-entry array is ample.
        if p_inside {
            output.points[output.count] = p;
            output.count += 1u;
        }
        if p_inside != q_inside {
            let t = (boundary - p_coordinate) / (q_coordinate - p_coordinate);
            output.points[output.count] = mix(p, q, t);
            output.count += 1u;
        }
        index += 1u;
    }
    return output;
}

fn triangle_pixel_coverage(
    a: vec2<f32>,
    b: vec2<f32>,
    c: vec2<f32>,
) -> f32 {
    var polygon: ClippedPolygon;
    polygon.count = 3u;
    polygon.points[0] = a;
    polygon.points[1] = b;
    polygon.points[2] = c;
    polygon = clip_polygon_axis(polygon, 0u, 0.0, true);
    polygon = clip_polygon_axis(polygon, 0u, 1.0, false);
    polygon = clip_polygon_axis(polygon, 1u, 0.0, true);
    polygon = clip_polygon_axis(polygon, 1u, 1.0, false);
    if polygon.count < 3u {
        return 0.0;
    }

    var twice_area = 0.0;
    var index = 0u;
    loop {
        if index >= polygon.count {
            break;
        }
        let next = select(index + 1u, 0u, index + 1u == polygon.count);
        let p = polygon.points[index];
        let q = polygon.points[next];
        twice_area += p.x * q.y - p.y * q.x;
        index += 1u;
    }
    return clamp(abs(twice_area) * 0.5, 0.0, 1.0);
}

@fragment
fn fs_path(input: PathVertexOutput) -> @location(0) vec4<f32> {
    // Fragment derivatives must execute in uniform control flow. `reveal` is an
    // interpolated input, so evaluate fwidth before any reveal-dependent branch.
    let edge = max(fwidth(input.path_progress), 0.00001);

    if input.reveal <= 0.0 {
        return vec4<f32>(0.0);
    }
    if input.exact_triangle != 0u {
        // The conservative quad covers every MSAA sample of candidate pixels.
        // Coverage is applied once from the exact pixel-box intersection rather
        // than multiplied by the hardware triangle sample mask.
        let pixel_minimum = floor(input.position.xy);
        let coverage = triangle_pixel_coverage(
            input.triangle_a - pixel_minimum,
            input.triangle_b - pixel_minimum,
            input.triangle_c - pixel_minimum,
        );
        let fill_alpha = smoothstep(0.0, 1.0, input.reveal);
        return input.color * coverage * fill_alpha;
    }
    if input.reveal >= 1.0 {
        return input.color;
    }

    if input.is_stroke < 0.5 {
        // Manim-like Create polish: reveal the border while smoothly bringing in
        // the authored fill instead of popping the complete fill on the last frame.
        let fill_alpha = smoothstep(0.0, 1.0, input.reveal);
        return input.color * fill_alpha;
    }

    let coverage = 1.0 - smoothstep(input.reveal, input.reveal + edge, input.path_progress);
    return input.color * coverage;
}
