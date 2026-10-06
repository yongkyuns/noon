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
