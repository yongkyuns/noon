struct GradientStopUniform {
    color: vec4<f32>,
    position: vec4<f32>,
};
struct GradientUniform {
    bounds: vec4<f32>,
    endpoints: vec4<f32>,
    metadata: vec4<u32>,
    transform: vec4<f32>,
    rotation: vec4<f32>,
    stops: array<GradientStopUniform, 16>,
};
@group(1) @binding(0) var<uniform> gradient: GradientUniform;

fn gradient_local_from_pixel(pixel: vec2<f32>) -> vec2<f32> {
    let clip = vec2<f32>(pixel.x * 2.0 / camera.viewport_size.x - 1.0,
        1.0 - pixel.y * 2.0 / camera.viewport_size.y);
    let delta = clip / camera.clip_scale + camera.center - gradient.transform.xy;
    let c = gradient.rotation.x;
    let s = gradient.rotation.y;
    return vec2<f32>(c * delta.x + s * delta.y, -s * delta.x + c * delta.y) / gradient.transform.zw;
}

// Return straight encoded-sRGB for the shared source quantizer. Interpolation
// itself is premultiplied so invisible RGB cannot leak into visible samples.
fn gradient_fill(local: vec2<f32>, carrier: vec4<f32>) -> vec4<f32> {
    let point = (local - gradient.bounds.xy) / gradient.bounds.zw;
    let axis = gradient.endpoints.zw - gradient.endpoints.xy;
    let extent = max(abs(axis.x), abs(axis.y));
    let scaled = axis / extent;
    let t = (dot(point - gradient.endpoints.xy, scaled) / dot(scaled, scaled)) / extent;
    let count = gradient.metadata.x;
    var color = gradient.stops[0].color;
    if t >= gradient.stops[0].position.x {
        color = gradient.stops[count - 1u].color;
        for (var i = 1u; i < count; i += 1u) {
            if t < gradient.stops[i].position.x {
                let a = gradient.stops[i - 1u];
                let b = gradient.stops[i];
                color = mix(a.color, b.color, (t - a.position.x) / (b.position.x - a.position.x));
                break;
            }
        }
    }
    if color.a <= 0.0 { return vec4<f32>(0.0); }
    return vec4<f32>(color.rgb / color.a, color.a * carrier.a);
}

