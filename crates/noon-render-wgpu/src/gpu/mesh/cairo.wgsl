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
