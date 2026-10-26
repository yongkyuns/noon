struct GradientStopUniform {
    color: vec4<f32>,
    position: vec4<f32>,
};
struct PaintUniform {
    bounds: vec4<f32>,
    endpoints: vec4<f32>,
    metadata: vec4<u32>,
    transform: vec4<f32>,
    rotation: vec4<f32>,
    stops: array<GradientStopUniform, 16>,
    image_u: vec4<f32>,
    image_v: vec4<f32>,
};
@group(1) @binding(0) var<uniform> paint: PaintUniform;
@group(2) @binding(0) var paint_image: texture_2d<f32>;

fn paint_local_from_pixel(pixel: vec2<f32>) -> vec2<f32> {
    let clip = vec2<f32>(pixel.x * 2.0 / camera.viewport_size.x - 1.0,
        1.0 - pixel.y * 2.0 / camera.viewport_size.y);
    let delta = clip / camera.clip_scale + camera.center - paint.transform.xy;
    let c = paint.rotation.x;
    let s = paint.rotation.y;
    return vec2<f32>(c * delta.x + s * delta.y, -s * delta.x + c * delta.y) / paint.transform.zw;
}

// Return straight encoded-sRGB for the shared source quantizer. Interpolation
// itself is premultiplied so invisible RGB cannot leak into visible samples.
fn paint_fill(local: vec2<f32>, carrier: vec4<f32>) -> vec4<f32> {
    let point = (local - paint.bounds.xy) / paint.bounds.zw;
    if paint.metadata.y == 1u {
        let uv = vec2(dot(paint.image_u.xyz, vec3(point, 1.0)), dot(paint.image_v.xyz, vec3(point, 1.0)));
        if paint.image_u.w == 1.0 && (any(uv < vec2(0.0)) || any(uv > vec2(1.0))) { return vec4(0.0); }
        let color = raster_sample(paint_image, uv, textureDimensions(paint_image), paint.metadata.z, paint.metadata.w, 1.0);
        if color.a <= 0.0 { return vec4(0.0); }
        return vec4(color.rgb / color.a, color.a * carrier.a);
    }
    let axis = paint.endpoints.zw - paint.endpoints.xy;
    let extent = max(abs(axis.x), abs(axis.y));
    let scaled = axis / extent;
    let t = (dot(point - paint.endpoints.xy, scaled) / dot(scaled, scaled)) / extent;
    let count = paint.metadata.x;
    var color = paint.stops[0].color;
    if t >= paint.stops[0].position.x {
        color = paint.stops[count - 1u].color;
        for (var i = 1u; i < count; i += 1u) {
            if t < paint.stops[i].position.x {
                let a = paint.stops[i - 1u];
                let b = paint.stops[i];
                color = mix(a.color, b.color, (t - a.position.x) / (b.position.x - a.position.x));
                break;
            }
        }
    }
    if color.a <= 0.0 { return vec4<f32>(0.0); }
    return vec4<f32>(color.rgb / color.a, color.a * carrier.a);
}
