// Linear associated radiance. A single float image per axis; no unit clamp or
// packed LDR reconstruction. Transparent padding retains full kernel normalization.
struct BlurUniform {
    size: vec2<u32>, radius: u32, reserved: u32,
    control: vec4<f32>, weights: array<vec4<f32>, 64>,
}
@group(0) @binding(0) var<uniform> blur: BlurUniform;
@group(0) @binding(1) var image: texture_2d<f32>;
@vertex fn vs_fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let p = array<vec2<f32>, 3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
    return vec4(p[index],0.0,1.0);
}
fn convolve(point: vec2<i32>, axis: vec2<i32>) -> vec4<f32> {
    var color = vec4(0.0);
    let radius = i32(blur.radius);
    for (var offset = -radius; offset <= radius; offset += 1) {
        let p = point + axis * offset;
        if all(p >= vec2(0)) && all(p < vec2<i32>(blur.size)) {
            let at = u32(abs(offset));
            color += textureLoad(image,p,0) * blur.weights[at / 4u][at % 4u];
        }
    }
    return color;
}
@fragment fn fs_horizontal(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return convolve(vec2<i32>(p.xy),vec2(1,0));
}
@fragment fn fs_vertical(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return convolve(vec2<i32>(p.xy),vec2(0,1));
}
@fragment fn fs_output(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(image,vec2<i32>(p.xy),0) * blur.control.x;
}
