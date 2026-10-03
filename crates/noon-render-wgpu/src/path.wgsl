struct Camera {
    center: vec2<f32>,
    clip_scale: vec2<f32>,
    viewport_size: vec2<f32>,
    padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

const POLYGON_COVERAGE_FLAG: u32 = 33554432u;
const POLYGON_QUAD_FLAG: u32 = 67108864u;
const POLYGON_CORNER_SHIFT: u32 = 27u;
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
    @location(11) polygon_b: vec4<f32>,
    @location(12) polygon_c: vec4<f32>,
    @location(13) polygon_d: vec4<f32>,
};

struct PathVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) path_progress: f32,
    @location(2) reveal: f32,
    @location(3) is_stroke: f32,
    @location(4) @interpolate(flat) polygon_a: vec2<f32>,
    @location(5) @interpolate(flat) polygon_b: vec2<f32>,
    @location(6) @interpolate(flat) polygon_c: vec2<f32>,
    @location(7) @interpolate(flat) polygon_d: vec2<f32>,
    @location(8) @interpolate(flat) polygon_count: u32,
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

// Ordinary tessellated paths never use polygon coverage. Keep their stage
// interface small instead of transporting four unused polygon corners and
// routing every fragment through the polygon clipping shader.
struct CompactPathVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) path_progress: f32,
    @location(2) reveal: f32,
    @location(3) is_stroke: f32,
};

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
    let exact_polygon = (input.surface_and_progress & POLYGON_COVERAGE_FLAG) != 0u;

    var output: PathVertexOutput;
    if exact_polygon {
        let a = world_to_pixel(transform_path_point(local, input));
        let b = world_to_pixel(transform_path_point(mix(input.polygon_b.xy, input.polygon_b.zw, morph), input));
        let c = world_to_pixel(transform_path_point(mix(input.polygon_c.xy, input.polygon_c.zw, morph), input));
        let d = world_to_pixel(transform_path_point(mix(input.polygon_d.xy, input.polygon_d.zw, morph), input));
        let is_quad = (input.surface_and_progress & POLYGON_QUAD_FLAG) != 0u;
        let minimum = min(a, min(b, min(c, d))) - vec2<f32>(1.0);
        let maximum = max(a, max(b, max(c, d))) + vec2<f32>(1.0);
        let corner = (input.surface_and_progress >> POLYGON_CORNER_SHIFT) & 3u;
        // Matches TRIANGLE_QUAD and TRIANGLE_QUAD_INDICES: lower-left,
        // lower-right, upper-right, upper-left.
        let quad_position = vec2<f32>(
            select(0.0, 1.0, corner == 1u || corner == 2u),
            select(0.0, 1.0, corner >= 2u),
        );
        let pixel = mix(minimum, maximum, quad_position);
        output.position = vec4<f32>(pixel_to_clip(pixel), 0.0, 1.0);
        output.polygon_a = a;
        output.polygon_b = b;
        output.polygon_c = c;
        output.polygon_d = d;
        output.polygon_count = select(3u, 4u, is_quad);
    } else {
        let world = transform_path_point(local, input);
        output.position = vec4<f32>((world - camera.center) * camera.clip_scale, 0.0, 1.0);
        output.polygon_a = vec2<f32>(0.0);
        output.polygon_b = vec2<f32>(0.0);
        output.polygon_c = vec2<f32>(0.0);
        output.polygon_d = vec2<f32>(0.0);
        output.polygon_count = 0u;
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
fn vs_path_compact(input: CompactPathVertexInput) -> CompactPathVertexOutput {
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
    var output: CompactPathVertexOutput;
    output.position = vec4<f32>((world - camera.center) * camera.clip_scale, 0.0, 1.0);
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

fn revealed_path_color(color: vec4<f32>, reveal: f32, is_stroke: f32, progress: f32, edge: f32) -> vec4<f32> {
    if reveal <= 0.0 {
        return vec4<f32>(0.0);
    }
    if reveal >= 1.0 {
        return color;
    }
    if is_stroke < 0.5 {
        // Bring in the fill while the authored border is revealed.
        return color * smoothstep(0.0, 1.0, reveal);
    }
    return color * (1.0 - smoothstep(reveal, reveal + edge, progress));
}

@fragment
fn fs_path_compact(input: CompactPathVertexOutput) -> @location(0) vec4<f32> {
    let edge = max(fwidth(input.path_progress), 0.00001);
    return revealed_path_color(input.color, input.reveal, input.is_stroke, input.path_progress, edge);
}

@fragment
fn fs_path(input: PathVertexOutput) -> @location(0) vec4<f32> {
    // Fragment derivatives must execute in uniform control flow. `reveal` is an
    // interpolated input, so evaluate fwidth before any reveal-dependent branch.
    let edge = max(fwidth(input.path_progress), 0.00001);

    if input.reveal <= 0.0 {
        return vec4<f32>(0.0);
    }
    if input.polygon_count != 0u {
        // The conservative quad covers every MSAA sample of candidate pixels.
        // Coverage is applied once from the exact pixel-box intersection rather
        // than multiplied by the hardware triangle sample mask.
        let pixel_minimum = floor(input.position.xy);
        let coverage = polygon_pixel_coverage(
            input.polygon_a - pixel_minimum,
            input.polygon_b - pixel_minimum,
            input.polygon_c - pixel_minimum,
            input.polygon_d - pixel_minimum,
            input.polygon_count,
        );
        let fill_alpha = smoothstep(0.0, 1.0, input.reveal);
        return input.color * coverage * fill_alpha;
    }
    return revealed_path_color(input.color, input.reveal, input.is_stroke, input.path_progress, edge);
}
