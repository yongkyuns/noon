// Pinned ManimCE 0.21 Cairo Surface appearance: lighting at two mapped path
// control points, with a projected, padded linear gradient between them.
struct CairoGeometry {
    p0: vec4<f32>,
    p6: vec4<f32>,
    span_p3_p0: vec4<f32>,
    span_p12_p0: vec4<f32>,
    span_p9_p6: vec4<f32>,
    span_p3_p6: vec4<f32>,
};
@group(1) @binding(0) var<uniform> cairo_geometry: CairoGeometry;
struct CairoOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) start: vec2<f32>,
    @location(1) @interpolate(flat) end: vec2<f32>,
    @location(2) @interpolate(flat) first_color: vec4<f32>,
    @location(3) @interpolate(flat) last_color: vec4<f32>,
};
fn max_component(value: vec3<f32>) -> f32 {
    return max(max(abs(value.x), abs(value.y)), abs(value.z));
}
// Match get_unit_normal's independent span scaling and aligned-vector fallback.
// Cross-product normals retain reflected and nonuniform world transforms.
fn cairo_normal(first: vec3<f32>, second: vec3<f32>) -> vec3<f32> {
    let a = max_component(first);
    let b = max_component(second);
    var u = vec3<f32>(0.0);
    if a == 0.0 {
        if b == 0.0 { return vec3<f32>(0.0, -1.0, 0.0); }
        u = second / b;
    } else if b == 0.0 {
        u = first / a;
    } else {
        u = first / a;
        let cp = cross(u, second / b);
        let magnitude = length(cp);
        if magnitude > 1e-6 { return cp / magnitude; }
    }
    if abs(u.x) < 1e-6 && abs(u.y) < 1e-6 {
        return vec3<f32>(0.0, -1.0, 0.0);
    }
    return stable_normalize(vec3<f32>(-u.x * u.z, -u.y * u.z,
        u.x * u.x + u.y * u.y)).xyz;
}
fn cairo_color(color: vec4<f32>, point: vec3<f32>, normal: vec3<f32>) -> vec4<f32> {
    var light = vec3<f32>(-7.0, -9.0, 10.0);
    if lighting.position_enabled.w > 0.5 { light = lighting.position_enabled.xyz; }
    let scale = max(max_component(light), max_component(point));
    var direction = vec3<f32>(0.0);
    if scale > 0.0 { direction = stable_normalize(light / scale - point / scale).xyz; }
    let cosine = dot(normal, direction);
    var response = 0.5 * cosine * cosine * cosine;
    if response < 0.0 { response *= 0.5; }
    // Cairo clamps each gradient stop before interpolating, rather than only
    // clamping the final fragment. Alpha is unchanged by illumination.
    return vec4<f32>(clamp(color.rgb + vec3<f32>(response), vec3<f32>(0.0),
        vec3<f32>(1.0)), color.a);
}
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
    return result;
}
@vertex fn vs_cairo(input: VertexInput) -> CairoOutput {
    let world = mat4x4<f32>(input.world0, input.world1, input.world2, input.world3);
    return cairo_output(camera.view_projection * world * vec4<f32>(input.position, 1.0),
        world, input.color);
}
@vertex fn vs_cairo_boundary(input: EdgeInput) -> CairoOutput {
    let world = mat4x4<f32>(input.world0, input.world1, input.world2, input.world3);
    return cairo_output(boundary_position(input), world, input.color);
}
@fragment fn fs_cairo(input: CairoOutput) -> @location(0) vec4<f32> {
    let delta = input.end - input.start;
    let scale = max(abs(delta.x), abs(delta.y));
    if !(scale > 0.0 && scale <= MAX_FINITE_F32) { return input.first_color; }
    let direction = delta / scale;
    let t = clamp(dot((input.position.xy - input.start) / scale, direction)
        / dot(direction, direction), 0.0, 1.0);
    return mix(input.first_color, input.last_color, t);
}
