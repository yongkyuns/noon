// Common encoded-premultiplied reconstruction for bitmap objects and paints.
// Addressing applies to each texel tap, before interpolation.
fn raster_pixel(image: texture_2d<f32>, point: vec2<f32>, dimensions: vec2<u32>, address: u32, opacity: f32) -> vec4<f32> {
    let size = vec2<f32>(dimensions);
    var p = point;
    if address == 0u {
        if any(p < vec2(0.0)) || any(p >= size) { return vec4(0.0); }
    } else if address == 2u {
        p -= floor(p / size) * size;
    }
    p = clamp(p, vec2(0.0), size - vec2(1.0));
    let rgba = textureLoad(image, vec2<i32>(p), 0);
    let alpha = rgba.a * opacity;
    return vec4(rgba.rgb * alpha, alpha);
}
fn raster_cubic(value: f32) -> f32 {
    let x = abs(value);
    if x <= 1.0 { return (x-2.0)*x*x+1.0; }
    if x < 2.0 { return ((-x+5.0)*x-8.0)*x+4.0; }
    return 0.0;
}
fn raster_sample(image: texture_2d<f32>, coordinates: vec2<f32>, dimensions: vec2<u32>, sampling: u32, address: u32, opacity: f32) -> vec4<f32> {
    var uv = coordinates;
    if address == 2u { uv = fract(uv); }
    let position = uv * vec2<f32>(dimensions) - vec2(0.5);
    let base = floor(position);
    let fraction = fract(position);
    if sampling == 0u {
        return raster_pixel(image, floor(uv * vec2<f32>(dimensions)), dimensions, address, opacity);
    }
    if sampling == 1u {
        let top = mix(raster_pixel(image, base, dimensions, address, opacity), raster_pixel(image, base+vec2(1.0,0.0), dimensions, address, opacity), fraction.x);
        let bottom = mix(raster_pixel(image, base+vec2(0.0,1.0), dimensions, address, opacity), raster_pixel(image, base+vec2(1.0,1.0), dimensions, address, opacity), fraction.x);
        return mix(top, bottom, fraction.y);
    }
    var color = vec4(0.0);
    for (var y = -1; y <= 2; y += 1) {
        for (var x = -1; x <= 2; x += 1) {
            color += raster_pixel(image, base+vec2(f32(x),f32(y)), dimensions, address, opacity) * raster_cubic(f32(x)-fraction.x) * raster_cubic(f32(y)-fraction.y);
        }
    }
    color.a = clamp(color.a, 0.0, 1.0);
    return vec4(clamp(color.rgb, vec3(0.0), vec3(color.a)), color.a);
}
