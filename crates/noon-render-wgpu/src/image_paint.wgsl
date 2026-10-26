// Image-paint coordinates and sampling operate on premultiplied encoded RGBA.
// The geometry pipeline still owns coverage, stroke and final object opacity.
struct ImagePaintUniform {
    bounds: vec4<f32>,
    fit: vec4<f32>,
    transform: vec4<f32>,
    rotation: vec4<f32>,
    uv_transform: vec4<f32>,
    uv_rotation: vec4<f32>,
    metadata: vec4<u32>,
};
@group(1) @binding(0) var<uniform> image_paint: ImagePaintUniform;
@group(2) @binding(0) var image_paint_texture: texture_2d<f32>;

fn image_paint_local_from_pixel(pixel: vec2<f32>) -> vec2<f32> {
    let viewport_pixel = pixel - camera.viewport_origin;
    let clip = vec2<f32>(viewport_pixel.x * 2.0 / camera.viewport_size.x - 1.0,
        1.0 - viewport_pixel.y * 2.0 / camera.viewport_size.y);
    let delta = clip / camera.clip_scale + camera.center - image_paint.transform.xy;
    let c = image_paint.rotation.x;
    let s = image_paint.rotation.y;
    return vec2<f32>(c * delta.x + s * delta.y, -s * delta.x + c * delta.y) / image_paint.transform.zw;
}

fn image_paint_pixel(point: vec2<i32>, dimensions: vec2<i32>) -> vec4<f32> {
    var at = point;
    if image_paint.metadata.y == 0u {
        if any(point < vec2<i32>(0)) || any(point >= dimensions) { return vec4<f32>(0.0); }
    } else if image_paint.metadata.y == 1u {
        at = clamp(point, vec2<i32>(0), dimensions - vec2<i32>(1));
    } else {
        at = ((point % dimensions) + dimensions) % dimensions;
    }
    let color = textureLoad(image_paint_texture, at, 0);
    return vec4<f32>(color.rgb * color.a, color.a);
}

fn image_paint_cubic(value: f32) -> f32 {
    let x = abs(value);
    if x <= 1.0 { return (x - 2.0) * x * x + 1.0; }
    if x < 2.0 { return ((-x + 5.0) * x - 8.0) * x + 4.0; }
    return 0.0;
}

fn image_paint_fill(local: vec2<f32>, carrier: vec4<f32>) -> vec4<f32> {
    let normalized = (local - image_paint.bounds.xy) / image_paint.bounds.zw;
    let delta = normalized - vec2<f32>(0.5) - image_paint.uv_transform.xy;
    let c = image_paint.uv_rotation.x;
    let s = image_paint.uv_rotation.y;
    let point = vec2<f32>(c * delta.x + s * delta.y, -s * delta.x + c * delta.y)
        / image_paint.uv_transform.zw + vec2<f32>(0.5);
    let fitted = (point - image_paint.fit.xy) / image_paint.fit.zw;
    if image_paint.metadata.x != 0u && (any(fitted < vec2<f32>(0.0)) || any(fitted > vec2<f32>(1.0))) {
        return vec4<f32>(0.0);
    }
    var uv = vec2<f32>(fitted.x, 1.0 - fitted.y);
    let dimensions = vec2<i32>(textureDimensions(image_paint_texture));
    let size = vec2<f32>(dimensions);
    if image_paint.metadata.y == 0u {
        let support = vec2<f32>(2.0) / size;
        if any(uv < -support) || any(uv > vec2<f32>(1.0) + support) { return vec4<f32>(0.0); }
    } else if image_paint.metadata.y == 1u {
        uv = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    } else {
        uv = fract(uv);
    }
    var color = vec4<f32>(0.0);
    if image_paint.metadata.z == 0u {
        color = image_paint_pixel(vec2<i32>(floor(uv * size)), dimensions);
    } else {
        let position = uv * size - vec2<f32>(0.5);
        let base = vec2<i32>(floor(position));
        let fraction = fract(position);
        if image_paint.metadata.z == 1u {
            let top = mix(image_paint_pixel(base, dimensions), image_paint_pixel(base + vec2<i32>(1, 0), dimensions), fraction.x);
            let bottom = mix(image_paint_pixel(base + vec2<i32>(0, 1), dimensions), image_paint_pixel(base + vec2<i32>(1, 1), dimensions), fraction.x);
            color = mix(top, bottom, fraction.y);
        } else {
            for (var y = -1; y <= 2; y += 1) {
                for (var x = -1; x <= 2; x += 1) {
                    color += image_paint_pixel(base + vec2<i32>(x, y), dimensions)
                        * image_paint_cubic(f32(x) - fraction.x) * image_paint_cubic(f32(y) - fraction.y);
                }
            }
        }
    }
    color.a = clamp(color.a, 0.0, 1.0);
    color = vec4<f32>(clamp(color.rgb, vec3<f32>(0.0), vec3<f32>(color.a)), color.a);
    if color.a <= 0.0 { return vec4<f32>(0.0); }
    return vec4<f32>(color.rgb / color.a, color.a * carrier.a);
}
