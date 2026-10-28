// Version-one seeded value noise in normalized source bounds. The mask scales
// every encoded-premultiplied channel; endpoint overrides are exact.
struct MaskUniform { control: vec4<f32>, domain: vec4<f32>, seed: vec4<u32> }
@group(0) @binding(0) var<uniform> mask: MaskUniform;
@group(0) @binding(1) var source: texture_2d<f32>;
@vertex fn vs_fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let points = array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));
    return vec4<f32>(points[index],0.0,1.0);
}
fn lattice(point: vec2<i32>) -> f32 {
    let p = bitcast<vec2<u32>>(point);
    var h = (p.x * 0x9e3779b9u) ^ (p.y * 0x85ebca6bu) ^ mask.seed.x;
    h = (h ^ (h >> 16u)) * 0x7feb352du;
    h = (h ^ (h >> 15u)) * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return f32(h >> 8u) / 16777216.0;
}
fn field(uv: vec2<f32>) -> f32 {
    let point = uv * 16.0;
    let base = vec2<i32>(floor(point));
    let f = fract(point);
    let w = f * f * (vec2<f32>(3.0) - 2.0 * f);
    return mix(mix(lattice(base), lattice(base+vec2<i32>(1,0)), w.x),
               mix(lattice(base+vec2<i32>(0,1)), lattice(base+vec2<i32>(1,1)), w.x), w.y);
}
// WGSL integer division truncates toward zero; source padding needs floor.
fn grid_floor_axis(value: i32, divisor: i32) -> i32 {
    // ES 3.0 leaves remainder with negative operands undefined. Keep the
    // division operands nonnegative on every backend, including padded cells.
    if value < 0 { return -((-value+divisor-1)/divisor); }
    return value/divisor;
}
fn grid_floor(value: vec2<i32>, divisor: i32) -> vec2<i32> {
    return vec2<i32>(grid_floor_axis(value.x,divisor),grid_floor_axis(value.y,divisor));
}
@fragment fn fs_sample(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    if mask.control.w == 1.0 {
        let cell = i32(mask.seed.x);
        if cell <= 256 { return textureLoad(source,vec2<i32>(position.xy),0) * mask.control.z; }
        let origin = bitcast<vec2<i32>>(mask.seed.yz);
        let pixel = vec2<i32>(position.xy)*256+vec2<i32>(128);
        let lower = origin + grid_floor(pixel-origin,cell)*cell;
        let sample = grid_floor(2*lower+vec2<i32>(cell),512);
        if any(sample < vec2<i32>(0)) || any(sample >= vec2<i32>(textureDimensions(source))) { return vec4<f32>(0.0); }
        return textureLoad(source,sample,0) * mask.control.z;
    }
    let original = textureLoad(source,vec2<i32>(position.xy),0);
    let progress = mask.control.x;
    if progress <= 0.0 { return original * mask.control.z; }
    if progress >= 1.0 { return vec4<f32>(0.0); }
    let noise = field((position.xy-mask.domain.xy)*mask.domain.zw);
    var coverage: f32;
    if mask.control.y == 0.0 {
        coverage = select(0.0, 1.0, noise >= progress);
    } else {
        let t = clamp((progress-noise)/mask.control.y+0.5,0.0,1.0);
        coverage = 1.0-t*t*(3.0-2.0*t);
    }
    return original * (coverage * mask.control.z);
}
