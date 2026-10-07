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
    return cairo_source_color(color, input.opacity);
}
