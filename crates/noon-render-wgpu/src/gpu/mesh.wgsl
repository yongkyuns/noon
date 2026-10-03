struct Camera {
    view_projection: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;
struct Lighting {
    position_enabled: vec4<f32>,
    color_intensity: vec4<f32>,
};
@group(0) @binding(1) var<uniform> lighting: Lighting;

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
const MAX_FINITE_F32: f32 = 3.402823e38;
// Normalize after scaling by the largest component. This avoids overflow in
// length-squared for large finite values and marks the zero vector explicitly.
fn stable_normalize(value: vec3<f32>) -> vec4<f32> {
    let scale = max(max(abs(value.x), abs(value.y)), abs(value.z));
    // WGSL has `isNan` and `isInf`, but no `isFinite`; ordered bounds reject
    // zero, infinities, and NaNs without relying on a nonstandard builtin.
    if !(scale > 0.0 && scale <= MAX_FINITE_F32) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    let scaled = value / scale;
    let magnitude = length(scaled);
    if !(magnitude > 0.0 && magnitude <= MAX_FINITE_F32) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    return vec4<f32>(scaled / magnitude, 1.0);
}
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
