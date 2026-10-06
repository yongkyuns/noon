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
