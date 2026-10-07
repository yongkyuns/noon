// Pinned ManimCE 0.21 Cairo Surface appearance: lighting at two mapped path
// control points, with a projected, padded linear gradient between them.
struct CairoGeometry {
    p0: vec4<f32>,
    p6: vec4<f32>,
    span_p3_p0: vec4<f32>,
    span_p12_p0: vec4<f32>,
    span_p9_p6: vec4<f32>,
    span_p3_p6: vec4<f32>,
    center: vec4<f32>,
    perimeter: array<vec4<f32>, 16>,
};
@group(1) @binding(0) var<uniform> cairo_geometry: CairoGeometry;
struct CairoOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) start: vec2<f32>,
    @location(1) @interpolate(flat) end: vec2<f32>,
    @location(2) @interpolate(flat) first_color: vec4<f32>,
    @location(3) @interpolate(flat) last_color: vec4<f32>,
    @location(4) @interpolate(flat) boundary_line: vec4<f32>,
    @location(5) @interpolate(flat) coverage_data: vec2<f32>,
    @location(6) @interpolate(flat) points0: vec4<f32>,
    @location(7) @interpolate(flat) points1: vec4<f32>,
    @location(8) @interpolate(flat) points2: vec4<f32>,
    @location(9) @interpolate(flat) points3: vec4<f32>,
    @location(10) @interpolate(flat) points4: vec4<f32>,
    @location(11) @interpolate(flat) points5: vec4<f32>,
    @location(12) @interpolate(flat) points6: vec4<f32>,
    @location(13) @interpolate(flat) points7: vec4<f32>,
};
fn cairo_output(position: vec4<f32>, world: mat4x4<f32>, color: vec4<f32>) -> CairoOutput {
    let start_world = (world * cairo_geometry.p0).xyz + world[3].xyz;
    let end_world = (world * cairo_geometry.p6).xyz + world[3].xyz;
    let n0 = cairo_normal((world * cairo_geometry.span_p3_p0).xyz,
        (world * cairo_geometry.span_p12_p0).xyz);
    let n1 = cairo_normal((world * cairo_geometry.span_p9_p6).xyz,
        (world * cairo_geometry.span_p3_p6).xyz);
    let a = camera.view_projection * vec4<f32>(start_world, 1.0);
    let b = camera.view_projection * vec4<f32>(end_world, 1.0);
    // Fragment coordinates use a top-left origin on both retained backends.
    let viewport = boundary_metrics.xy;
    var result: CairoOutput;
    result.position = position;
    result.start = (a.xy / a.w * vec2<f32>(1.0, -1.0) + vec2<f32>(1.0)) * viewport * 0.5;
    result.end = (b.xy / b.w * vec2<f32>(1.0, -1.0) + vec2<f32>(1.0)) * viewport * 0.5;
    result.first_color = cairo_color(color, start_world, n0);
    result.last_color = cairo_color(color, end_world, n1);
    result.boundary_line = vec4<f32>(0.0);
    return result;
}
fn cairo_perimeter_point(input: CairoOutput, index: u32) -> vec2<f32> {
    var pair: vec4<f32>;
    switch index / 2u {
        case 0u: { pair = input.points0; }
        case 1u: { pair = input.points1; }
        case 2u: { pair = input.points2; }
        case 3u: { pair = input.points3; }
        case 4u: { pair = input.points4; }
        case 5u: { pair = input.points5; }
        case 6u: { pair = input.points6; }
        default: { pair = input.points7; }
    }
    return select(pair.xy, pair.zw, index % 2u == 1u);
}
fn cairo_set_perimeter_pair(output: ptr<function, CairoOutput>, index: u32, pair: vec4<f32>) {
    switch index {
        case 0u: { (*output).points0 = pair; }
        case 1u: { (*output).points1 = pair; }
        case 2u: { (*output).points2 = pair; }
        case 3u: { (*output).points3 = pair; }
        case 4u: { (*output).points4 = pair; }
        case 5u: { (*output).points5 = pair; }
        case 6u: { (*output).points6 = pair; }
        default: { (*output).points7 = pair; }
    }
}
fn cairo_cross(a: vec2<f32>, b: vec2<f32>) -> f32 {
    return a.x * b.y - a.y * b.x;
}
fn cairo_inward_normal(center: vec2<f32>, point: vec2<f32>, neighbor: vec2<f32>) -> vec2<f32> {
    let edge = neighbor - point;
    let edge_length = length(edge);
    if edge_length <= 1e-6 { return vec2<f32>(0.0); }
    let normal = vec2<f32>(-edge.y, edge.x) / edge_length;
    return select(normal, -normal, dot(normal, center - point) < 0.0);
}
@vertex fn vs_cairo(input: VertexInput, @builtin(vertex_index) vertex: u32) -> CairoOutput {
    let world = mat4x4<f32>(input.world0, input.world1, input.world2, input.world3);
    let transform = camera.view_projection * world;
    var position = transform * vec4<f32>(input.position, 1.0);
    var result = cairo_output(position, world, input.color);
    let count = u32(cairo_geometry.center.w);
    let scale = boundary_metrics.xy * 0.5;
    var lower = vec2<f32>(MAX_FINITE_F32);
    var upper = -lower;
    var valid = true;
    for (var index = 0u; index < 8u; index += 1u) {
        if index * 2u >= count { break; }
        let a = transform * cairo_geometry.perimeter[index * 2u];
        let b = transform * cairo_geometry.perimeter[min(index * 2u + 1u, count - 1u)];
        valid = valid && min(a.w, b.w) > 1e-6;
        let pa = a.xy / max(a.w, 1e-6) * scale;
        let pb = b.xy / max(b.w, 1e-6) * scale;
        lower = min(lower, min(pa, pb));
        upper = max(upper, max(pa, pb));
        cairo_set_perimeter_pair(&result, index, vec4<f32>(
            vec2<f32>(pa.x + scale.x, scale.y - pa.y),
            vec2<f32>(pb.x + scale.x, scale.y - pb.y)));
    }
    result.coverage_data = vec2<f32>(select(0.0, f32(count), valid), 0.0);
    var positive = false;
    var negative = false;
    // Four consistent turns prove the simple quad fast path. Curved rings
    // retain the complete signed integral, including overlapping fan pieces.
    for (var index = 0u; index < 4u; index += 1u) {
        if count != 4u { break; }
        let a = cairo_perimeter_point(result, index);
        let b = cairo_perimeter_point(result, (index + 1u) % count);
        let c = cairo_perimeter_point(result, (index + 2u) % count);
        let turn = cairo_cross(b - a, c - b);
        positive = positive || turn > 0.0;
        negative = negative || turn < 0.0;
    }
    result.coverage_data.y = select(0.0, 1.0, count == 4u && !(positive && negative));
    if count > 0u && vertex > 0u && valid {
        let previous = cairo_geometry.perimeter[(vertex + count - 2u) % count];
        let next = cairo_geometry.perimeter[vertex % count];
        let center = transform * vec4<f32>(cairo_geometry.center.xyz, 1.0);
        let before = transform * previous;
        let after = transform * next;
        if min(min(position.w, center.w), min(before.w, after.w)) > 1e-6 {
            let point = position.xy / position.w * scale;
            let origin = center.xy / center.w * scale;
            let n0 = cairo_inward_normal(origin, point, before.xy / before.w * scale);
            let n1 = cairo_inward_normal(origin, point, after.xy / after.w * scale);
            let normal = n0 + n1;
            let denominator = max(dot(normal, n0), dot(normal, n1));
            var offset = vec2<f32>(0.0);
            if denominator > 1e-6 { offset = -normal * (1.25 / denominator); }
            // Include the pixel filter and multisample locations, but never
            // let a thin fan extend beyond the face's padded screen bounds.
            let expanded = clamp(point + offset,
                lower - vec2<f32>(1.0), upper + vec2<f32>(1.0));
            position = vec4<f32>(expanded / scale * position.w,
                position.z, position.w);
        }
    }
    result.position = position;
    return result;
}
@vertex fn vs_cairo_boundary(input: EdgeInput) -> CairoOutput {
    let world = mat4x4<f32>(input.world0, input.world1, input.world2, input.world3);
    let geometry = boundary_geometry(input);
    var result = cairo_output(geometry.position, world, input.color);
    result.boundary_line = geometry.line;
    return result;
}
fn cairo_fragment_color(input: CairoOutput) -> vec4<f32> {
    let delta = input.end - input.start;
    let scale = max(abs(delta.x), abs(delta.y));
    if !(scale > 0.0 && scale <= MAX_FINITE_F32) { return input.first_color; }
    let direction = delta / scale;
    let t = clamp(dot((input.position.xy - input.start) / scale, direction)
        / dot(direction, direction), 0.0, 1.0);
    return mix(input.first_color, input.last_color, t);
}

@fragment fn fs_cairo(input: CairoOutput) -> @location(0) vec4<f32> {
    let color = cairo_fragment_color(input);
    let count = u32(input.coverage_data.x);
    var coverage = 1.0;
    if count > 0u {
        let origin = input.position.xy - vec2<f32>(0.5);
        let a = cairo_perimeter_point(input, 0u) - origin;
        if count == 4u && input.coverage_data.y == 1.0 {
            coverage = polygon_pixel_coverage(a, cairo_perimeter_point(input, 1u) - origin,
                cairo_perimeter_point(input, 2u) - origin,
                cairo_perimeter_point(input, 3u) - origin, 4u);
        } else {
            // Integrate the complete perimeter once. Opposite edges of subpixel
            // cells and concave regions filter together without fan clipping.
            var area = 0.0;
            var previous = cairo_perimeter_point(input, count - 1u) - origin;
            for (var index = 0u; index < 16u; index += 1u) {
                if index >= count { break; }
                let point = cairo_perimeter_point(input, index) - origin;
                area += polygon_edge_pixel_area(previous, point);
                previous = point;
            }
            coverage = clamp(abs(area), 0.0, 1.0);
        }
    }
    if coverage == 0.0 { discard; }
    return vec4<f32>(color.rgb, color.a * coverage);
}
@fragment fn fs_cairo_boundary(input: CairoOutput) -> @location(0) vec4<f32> {
    let color = cairo_fragment_color(input);
    return vec4<f32>(color.rgb,
        color.a * boundary_coverage(input.position.xy, input.boundary_line));
}
