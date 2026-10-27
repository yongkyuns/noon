// Painted alpha, Gaussian-filtered then translated with transparent bilinear
// extension. Original encoded-premultiplied color paints over the shadow.
struct ShadowUniform { tint: vec4<f32>, offset_opacity: vec4<f32> }
@group(0) @binding(0) var<uniform> shadow: ShadowUniform;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var blurred: texture_2d<f32>;
@vertex fn vs_fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let points = array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));
    return vec4<f32>(points[index],0.0,1.0);
}
fn alpha_at(point: vec2<i32>) -> f32 {
    if any(point < vec2<i32>(0)) || any(point >= vec2<i32>(textureDimensions(blurred))) { return 0.0; }
    return textureLoad(blurred, point, 0).a;
}
@fragment fn fs_shadow(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel=vec2<i32>(position.xy);
    let point=vec2<f32>(pixel)-shadow.offset_opacity.xy;
    let base=vec2<i32>(floor(point));
    let f=fract(point);
    let alpha=mix(mix(alpha_at(base),alpha_at(base+vec2<i32>(1,0)),f.x),mix(alpha_at(base+vec2<i32>(0,1)),alpha_at(base+vec2<i32>(1,1)),f.x),f.y)*shadow.tint.a;
    let original=textureLoad(source,pixel,0);
    return (original+(1.0-original.a)*vec4<f32>(shadow.tint.rgb*alpha,alpha))*shadow.offset_opacity.z;
}
