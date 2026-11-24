// All intermediates are linear, alpha-associated RGBA16Float radiance.
struct BloomUniform {
    size: vec2<u32>, radius: u32, reserved: u32,
    control: vec4<f32>, // luminance threshold, intensity, reserved
    weights: array<vec4<f32>, 64>,
}
@group(0) @binding(0) var<uniform> bloom: BloomUniform;
@group(0) @binding(1) var primary: texture_2d<f32>;
@group(0) @binding(2) var secondary: texture_2d<f32>;
@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(positions[index], 0.0, 1.0);
}
@fragment
fn fs_extract(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let source = textureLoad(primary, vec2<i32>(position.xy), 0);
    let luminance = dot(source.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    if luminance <= bloom.control.x { return vec4<f32>(0.0); }
    return source * ((luminance - bloom.control.x) / luminance);
}
fn coefficient(offset: i32) -> f32 {
    let at = u32(abs(offset));
    return bloom.weights[at / 4u][at % 4u];
}
fn gaussian(point: vec2<i32>, axis: vec2<i32>) -> vec4<f32> {
    var color = vec4<f32>(0.0);
    let radius = i32(bloom.radius);
    for (var offset = -radius; offset <= radius; offset += 1) {
        let sample_at = point + offset * axis;
        if all(sample_at >= vec2<i32>(0)) && all(sample_at < vec2<i32>(bloom.size)) {
            color += textureLoad(primary, sample_at, 0) * coefficient(offset);
        }
    }
    return color;
}
@fragment
fn fs_horizontal(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return gaussian(vec2<i32>(position.xy), vec2<i32>(1, 0));
}
@fragment
fn fs_vertical(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return gaussian(vec2<i32>(position.xy), vec2<i32>(0, 1));
}
@fragment
fn fs_combine(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let point = vec2<i32>(position.xy);
    let base = textureLoad(primary, point, 0);
    if bloom.control.y == 0.0 { return base; }
    let added = textureLoad(secondary, point, 0) * bloom.control.y;
    let alpha = base.a + (1.0 - base.a) * clamp(added.a, 0.0, 1.0);
    return vec4<f32>(base.rgb + added.rgb, alpha);
}
fn srgb_encode(value: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(value, vec3<f32>(1.0 / 2.4)) - 0.055, 12.92 * value, value <= vec3<f32>(0.0031308));
}
@fragment
fn fs_resolve(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let radiance = textureLoad(primary, vec2<i32>(position.xy), 0);
    if radiance.a == 0.0 { return vec4<f32>(0.0); }
    let straight = radiance.rgb / radiance.a;
    let mapped = straight / (vec3<f32>(1.0) + straight);
    return vec4<f32>(srgb_encode(mapped) * radiance.a, radiance.a);
}
