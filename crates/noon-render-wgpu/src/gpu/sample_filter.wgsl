// Version-one seeded value noise in normalized source bounds. The mask scales
// every encoded-premultiplied channel; endpoint overrides are exact.
struct MaskUniform {
    control: vec4<f32>, domain: vec4<f32>, seed: vec4<u32>,
    radial: vec4<f32>, edge: vec4<f32>, inner: vec4<f32>, outer: vec4<f32>
}
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
// Radial controls use normalized upright capture bounds. The footprint is
// resolved from physical output pixels, so neither derivatives nor a host clock
// are required. Under-resolved roughness fades to its zero-mean field.
fn radial_sample(position: vec2<f32>, original: vec4<f32>, progress: f32) -> vec4<f32> {
    let local = (position-mask.domain.xy)*mask.domain.zw;
    let uv = vec2<f32>(local.x, 1.0-local.y);
    let center = mask.radial.xy;
    let axes = mask.radial.zw;
    let q = (uv-center)/axes;
    let furthest = max(center, vec2<f32>(1.0)-center)/axes;
    let maximum = length(furthest);
    let footprint = length(mask.domain.zw/axes);
    let noise_footprint = max(mask.domain.z, mask.domain.w)*mask.edge.y;
    let roughness = mask.edge.x * (1.0-smoothstep(0.5,1.0,noise_footprint));
    let distance = length(q) + roughness*(2.0*field(uv*(mask.edge.y/16.0))-1.0);
    let width = max(mask.control.y, footprint);
    let margin = mask.edge.x + width + mask.edge.z;
    let opening = select(1.0-progress, progress, mask.edge.w > 0.5);
    let radius = mix(-margin, maximum+margin, opening);
    let signed_distance = select(radius-distance, distance-radius, mask.edge.w > 0.5);
    let coverage = smoothstep(-0.5*width, 0.5*width, signed_distance);
    var color = original.rgb;
    if mask.edge.z > 0.0 {
        let phase = clamp(signed_distance/mask.edge.z, 0.0, 1.0);
        // Interpolate rim colors in encoded premultiplied space, independently
        // of source opacity. Rim alpha controls tint strength, never new coverage.
        let rim_alpha = mix(mask.inner.a, mask.outer.a, phase);
        let rim_rgb = mix(mask.inner.rgb*mask.inner.a, mask.outer.rgb*mask.outer.a, phase);
        let band = 1.0-smoothstep(max(0.0,mask.edge.z-footprint),mask.edge.z+footprint,signed_distance);
        color = original.rgb*(1.0-band*rim_alpha) + rim_rgb*original.a*band;
    }
    return vec4<f32>(color,original.a)*(coverage*mask.control.z);
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
    if mask.control.w == 2.0 { return radial_sample(position.xy,original,progress); }
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
