struct Camera {
    view_projection: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;
struct Lighting {
    position_enabled: vec4<f32>,
    color_intensity: vec4<f32>,
};
@group(0) @binding(1) var<uniform> lighting: Lighting;
@group(0) @binding(2) var<uniform> boundary_metrics: vec4<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) world0: vec4<f32>,
    @location(3) world1: vec4<f32>,
    @location(4) world2: vec4<f32>,
    @location(5) world3: vec4<f32>,
    @location(6) normal0: vec4<f32>,
    @location(7) normal1: vec4<f32>,
    @location(8) normal2: vec4<f32>,
    @location(9) color: vec4<f32>,
};
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world_position: vec3<f32>,
    @location(2) world_normal: vec3<f32>,
    @location(3) point_lit: f32,
};
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    let world = mat4x4<f32>(input.world0, input.world1, input.world2, input.world3);
    let normal_matrix = mat3x3<f32>(input.normal0.xyz, input.normal1.xyz, input.normal2.xyz);
    let world_position = world * vec4<f32>(input.position, 1.0);
    var output: VertexOutput;
    // Keep the established unlit clip-space evaluation order bit-for-bit.
    output.position = camera.view_projection * world * vec4<f32>(input.position, 1.0);
    output.color = input.color;
    output.world_position = world_position.xyz;
    output.world_normal = normal_matrix * input.normal;
    output.point_lit = input.normal0.w;
    return output;
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var result = input.color;
    if input.point_lit > 0.5 && lighting.position_enabled.w > 0.5 {
        let normal = stable_normalize(input.world_normal);
        // Scale the positions before subtraction as well, so opposite large
        // finite positions cannot overflow while computing the direction.
        let position_scale = max(
            max(max(abs(lighting.position_enabled.x), abs(lighting.position_enabled.y)), abs(lighting.position_enabled.z)),
            max(max(abs(input.world_position.x), abs(input.world_position.y)), abs(input.world_position.z)),
        );
        var toward_light = vec3<f32>(0.0, 0.0, 0.0);
        if position_scale > 0.0 && position_scale <= MAX_FINITE_F32 {
            toward_light = lighting.position_enabled.xyz / position_scale - input.world_position / position_scale;
        }
        let direction = stable_normalize(toward_light);
        if normal.w > 0.5 && direction.w > 0.5 {
            let cosine = dot(normal.xyz, direction.xyz);
            let cubic = cosine * cosine * cosine;
            var response = 0.25 * cubic;
            if cosine >= 0.0 {
                response = 0.5 * cubic;
            }
            result = vec4<f32>(result.rgb + lighting.color_intensity.rgb * lighting.color_intensity.a * response, result.a);
        }
    }
    return result;
}

struct EdgeInput {
    @location(0) start: vec3<f32>,
    @location(1) end: vec3<f32>,
    @location(10) corner: vec2<f32>,
    @location(2) world0: vec4<f32>,
    @location(3) world1: vec4<f32>,
    @location(4) world2: vec4<f32>,
    @location(5) world3: vec4<f32>,
    @location(6) normal0: vec4<f32>,
    @location(7) normal1: vec4<f32>,
    @location(8) normal2: vec4<f32>,
    @location(9) color: vec4<f32>,
};
struct EdgeOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};
fn boundary_position(input: EdgeInput) -> vec4<f32> {
    let world = mat4x4<f32>(input.world0, input.world1, input.world2, input.world3);
    var a = camera.view_projection * world * vec4<f32>(input.start, 1.0);
    var b = camera.view_projection * world * vec4<f32>(input.end, 1.0);
    // Clip the centerline before dividing by W. Fully clipped segments collapse
    // instead of creating infinities during billboard extrusion.
    if a.z < 0.0 && b.z < 0.0 {
        a = vec4<f32>(0.0, 0.0, -1.0, 1.0);
        b = a;
    } else {
        if a.z < 0.0 { a = mix(a, b, a.z / (a.z - b.z)); }
        if b.z < 0.0 { b = mix(b, a, b.z / (b.z - a.z)); }
    }
    let viewport = boundary_metrics.xy;
    let delta = (b.xy / max(b.w, 1e-8) - a.xy / max(a.w, 1e-8))
        * (viewport / max(viewport.x, viewport.y));
    let scale = max(abs(delta.x), abs(delta.y));
    var perpendicular = vec2<f32>(0.0, 0.0);
    if scale > 1e-8 && scale <= MAX_FINITE_F32 {
        let direction = delta / scale;
        perpendicular = vec2<f32>(-direction.y, direction.x) / length(direction);
    }
    var p = mix(a, b, input.corner.x);
    // The width is in authoring frame units; projecting endpoints and extruding
    // stays on the GPU even while the camera moves.
    p = vec4<f32>(p.xy + perpendicular * input.corner.y * input.normal1.w
        * boundary_metrics.zw * 0.5 * p.w, p.zw);
    return p;
}
@vertex fn vs_boundary(input: EdgeInput) -> EdgeOutput {
    var result: EdgeOutput;
    result.position = boundary_position(input);
    result.color = input.color;
    return result;
}
@fragment fn fs_boundary(input: EdgeOutput) -> @location(0) vec4<f32> {
    return input.color;
}
