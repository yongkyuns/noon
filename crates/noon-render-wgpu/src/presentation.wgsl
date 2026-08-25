@group(0) @binding(0)
var scene_texture: texture_2d<f32>;

@vertex
fn vs_present(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[vertex_index], 0.0, 1.0);
}

fn srgb_to_linear_channel(encoded: f32) -> f32 {
    if encoded <= 0.04045 {
        return encoded / 12.92;
    }
    return pow((encoded + 0.055) / 1.055, 2.4);
}

fn srgb_to_linear(encoded: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        srgb_to_linear_channel(encoded.r),
        srgb_to_linear_channel(encoded.g),
        srgb_to_linear_channel(encoded.b),
    );
}

@fragment
fn fs_present(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let dimensions = textureDimensions(scene_texture);
    let pixel = clamp(
        vec2<i32>(position.xy),
        vec2<i32>(0),
        vec2<i32>(dimensions) - vec2<i32>(1),
    );
    let encoded = textureLoad(scene_texture, pixel, 0);

    // Noon's semantic colors intentionally carry their display/sRGB encoding all
    // the way through scene blending. WebGL's browser drawing buffer applies an
    // unavoidable sRGB store transfer, unlike the WebGPU UNORM presentation path.
    // Decode only at the final presentation boundary so that the browser's store
    // transfer reconstructs the exact encoded scene bytes without changing any
    // fill/stroke/alpha blending inside the scene render.
    return vec4<f32>(srgb_to_linear(encoded.rgb), encoded.a);
}
