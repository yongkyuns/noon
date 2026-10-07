struct Lighting {
    position_enabled: vec4<f32>,
    color_intensity: vec4<f32>,
};
@group(0) @binding(1) var<uniform> lighting: Lighting;
@group(0) @binding(2) var<uniform> boundary_metrics: vec4<f32>;
struct CairoPath {
    p0: vec4<f32>,
    p6: vec4<f32>,
    start_next: vec4<f32>,
    start_previous: vec4<f32>,
    end_next: vec4<f32>,
    end_previous: vec4<f32>,
    gradient_start: vec4<f32>,
    gradient_end: vec4<f32>,
    metadata: vec4<f32>,
};
@group(2) @binding(0) var<uniform> cairo_path: CairoPath;
struct CairoPathOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) start: vec2<f32>,
    @location(1) @interpolate(flat) end: vec2<f32>,
    @location(2) @interpolate(flat) first_color: vec4<f32>,
    @location(3) @interpolate(flat) last_color: vec4<f32>,
    @location(4) opacity: f32,
    @location(5) @interpolate(flat) line_points0: vec4<f32>,
    @location(6) @interpolate(flat) line_points1: vec4<f32>,
    @location(7) @interpolate(flat) filtered_line: f32,
};
@vertex fn vs_cairo(input: VertexInput) -> CairoPathOutput {
    let world = mat4x4<f32>(input.world0, input.world1, input.world2, input.world3);
    let a_world = (world * cairo_path.p0).xyz + world[3].xyz;
    let b_world = (world * cairo_path.p6).xyz + world[3].xyz;
    var n0 = vec3<f32>(0.0, 1.0, 0.0);
    var n1 = n0;
    if cairo_path.metadata.y == 0.0 {
        n0 = cairo_normal((world * cairo_path.start_next).xyz,
            (world * cairo_path.start_previous).xyz);
        n1 = cairo_normal((world * cairo_path.end_next).xyz,
            (world * cairo_path.end_previous).xyz);
    }
    var start = a_world;
    var end = b_world;
    if cairo_path.metadata.z > 0.5 {
        start = cairo_path.gradient_start.xyz;
        end = cairo_path.gradient_end.xyz;
    }
    let a = camera.view_projection * vec4<f32>(start, 1.0);
    let b = camera.view_projection * vec4<f32>(end, 1.0);
    let base = select(input.fill, input.stroke, (input.surface & 1u) != 0u);
    let sheen = vec4<f32>(clamp(base.rgb + vec3<f32>(cairo_path.metadata.x),
        vec3<f32>(0.0), vec3<f32>(1.0)), base.a);
    var result: CairoPathOutput;
    // Cairo uses a 10x miter limit; this offset measures half the full miter.
    result.position = path_vertex(input, 5.0).position;
    result.start = (a.xy / a.w * vec2<f32>(1.0, -1.0) + vec2<f32>(1.0)) * boundary_metrics.xy * 0.5;
    result.end = (b.xy / b.w * vec2<f32>(1.0, -1.0) + vec2<f32>(1.0)) * boundary_metrics.xy * 0.5;
    result.first_color = cairo_color(base, a_world, n0);
    result.last_color = cairo_color(sheen, b_world, n1);
    result.opacity = input.opacity;
    // Only retained straight, screen-space butt strokes use this filter.
    // Curves, joins, other caps and clipped endpoints retain their old path.
    if cairo_path.metadata.w == 1.0 && (input.surface & 1u) != 0u
        && input.fixed_orientation == 0u {
        let transform = camera.view_projection * world;
        let first = transform * vec4<f32>(cairo_path.p0.xyz, 1.0);
        let last = transform * vec4<f32>(
            cairo_path.p0.xyz + cairo_path.start_next.xyz, 1.0);
        let center = transform * vec4<f32>(input.local, 0.0, 1.0);
        if min(first.w, last.w) > 1e-6 && min(first.z, last.z) >= 0.0
            && first.z <= first.w && last.z <= last.w {
            let pixel_scale = boundary_metrics.xy * vec2<f32>(0.5, -0.5);
            let pixel_origin = boundary_metrics.xy * 0.5;
            let p0 = first.xy / first.w * pixel_scale + pixel_origin;
            let p1 = last.xy / last.w * pixel_scale + pixel_origin;
            let delta = p1 - p0;
            let magnitude = length(delta);
            if magnitude > 1e-6 && magnitude <= MAX_FINITE_F32 {
                let direction = delta / magnitude;
                let normal = vec2<f32>(-direction.y, direction.x);
                let middle = center.xy / center.w * pixel_scale + pixel_origin;
                let original = result.position.xy / result.position.w
                    * pixel_scale + pixel_origin;
                // Preserve the existing width conversion, including viewport
                // anisotropy; only its box-filter coverage changes.
                let side = select(-1.0, 1.0, input.extrusion.y > 0.0);
                let stroke_offset = (original - middle) * side;
                result.line_points0 = vec4<f32>(
                    p0 - stroke_offset, p1 - stroke_offset);
                result.line_points1 = vec4<f32>(
                    p1 + stroke_offset, p0 + stroke_offset);
                result.filtered_line = 1.0;
                let at_end = dot(middle - p0, delta) > dot(delta, delta) * 0.5;
                let along = select(-1.25, 1.25, at_end);
                let padding = normal * select(
                    -1.25, 1.25, dot(stroke_offset, normal) > 0.0);
                let expanded = select(p0, p1, at_end) + direction * along
                    + (stroke_offset + padding) * side;
                result.position = vec4<f32>(
                    (expanded - pixel_origin) / pixel_scale * center.w,
                    center.z, center.w);
            }
        }
    }
    return result;
}
@fragment fn fs_cairo(input: CairoPathOutput) -> @location(0) vec4<f32> {
    let delta = input.end - input.start;
    let scale = max(abs(delta.x), abs(delta.y));
    var color = input.first_color;
    if scale > 0.0 && scale <= MAX_FINITE_F32 {
        let direction = delta / scale;
        let t = clamp(dot((input.position.xy - input.start) / scale, direction)
            / dot(direction, direction), 0.0, 1.0);
        color = mix(input.first_color, input.last_color, t);
    }
    var output = cairo_source_color(color, input.opacity);
    if input.filtered_line == 1.0 {
        let origin = input.position.xy - vec2<f32>(0.5);
        let coverage = polygon_pixel_coverage(
            input.line_points0.xy - origin, input.line_points0.zw - origin,
            input.line_points1.xy - origin, input.line_points1.zw - origin, 4u);
        if coverage == 0.0 { discard; }
        output *= coverage;
    }
    return output;
}
