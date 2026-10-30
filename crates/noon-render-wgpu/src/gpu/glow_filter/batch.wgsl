// glow-encoded-ldr-v1, full-resolution finite Gaussian. These are renderer
// passes, not a clock, scene representation, or alternate effects runtime.
// The mask is packed into RGB24 in portable RGBA8 render attachments. Do not
// reduce it to one 8-bit channel: faint halos are amplified by intensity.
struct GlowUniform {
    size: vec2<u32>,
    radius: u32,
    reserved: u32,
    tint: vec4<f32>,
    control: vec4<f32>, // intensity, final scope opacity, reserved, reserved
    weights: array<vec4<f32>, 64>, // nonnegative offsets 0..192, vec4 UBO stride
}

@group(0) @binding(0) var<uniform> glow0: GlowUniform;
@group(0) @binding(1) var image0: texture_2d<f32>;

@group(1) @binding(0) var<uniform> glow1: GlowUniform;
@group(1) @binding(1) var image1: texture_2d<f32>;

@group(2) @binding(0) var<uniform> glow2: GlowUniform;
@group(2) @binding(1) var image2: texture_2d<f32>;

@group(3) @binding(0) var<uniform> glow3: GlowUniform;
@group(3) @binding(1) var image3: texture_2d<f32>;

@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[index], 0.0, 1.0);
}

fn pack_mask(value: f32) -> vec4<f32> {
    let bits = u32(round(clamp(value, 0.0, 1.0) * 16777215.0));
    return vec4<f32>(
        f32((bits >> 16u) & 255u), f32((bits >> 8u) & 255u),
        f32(bits & 255u), 255.0,
    ) / 255.0;
}

fn unpack_mask(value: vec4<f32>) -> f32 {
    let bytes = round(value.rgb * 255.0);
    return dot(bytes, vec3<f32>(65536.0, 256.0, 1.0)) / 16777215.0;
}


fn blur0(point: vec2<i32>, vertical: bool) -> vec4<f32> {
    var value = 0.0;
    for (var offset = -i32(glow0.radius); offset <= i32(glow0.radius); offset += 1) {
        let sample_point = point + select(vec2<i32>(offset, 0), vec2<i32>(0, offset), vertical);
        if all(sample_point >= vec2<i32>(0)) && all(sample_point < vec2<i32>(glow0.size)) {
            let index = u32(abs(offset));
            let weight = glow0.weights[index / 4u][index % 4u];
            let sample_value = textureLoad(image0, sample_point, 0);
            var alpha = sample_value.a;
            if vertical { alpha = unpack_mask(sample_value); }
            value += weight * alpha;
        }
    }
    return pack_mask(value);
}

fn blur1(point: vec2<i32>, vertical: bool) -> vec4<f32> {
    var value = 0.0;
    for (var offset = -i32(glow1.radius); offset <= i32(glow1.radius); offset += 1) {
        let sample_point = point + select(vec2<i32>(offset, 0), vec2<i32>(0, offset), vertical);
        if all(sample_point >= vec2<i32>(0)) && all(sample_point < vec2<i32>(glow1.size)) {
            let index = u32(abs(offset));
            let weight = glow1.weights[index / 4u][index % 4u];
            let sample_value = textureLoad(image1, sample_point, 0);
            var alpha = sample_value.a;
            if vertical { alpha = unpack_mask(sample_value); }
            value += weight * alpha;
        }
    }
    return pack_mask(value);
}

fn blur2(point: vec2<i32>, vertical: bool) -> vec4<f32> {
    var value = 0.0;
    for (var offset = -i32(glow2.radius); offset <= i32(glow2.radius); offset += 1) {
        let sample_point = point + select(vec2<i32>(offset, 0), vec2<i32>(0, offset), vertical);
        if all(sample_point >= vec2<i32>(0)) && all(sample_point < vec2<i32>(glow2.size)) {
            let index = u32(abs(offset));
            let weight = glow2.weights[index / 4u][index % 4u];
            let sample_value = textureLoad(image2, sample_point, 0);
            var alpha = sample_value.a;
            if vertical { alpha = unpack_mask(sample_value); }
            value += weight * alpha;
        }
    }
    return pack_mask(value);
}

fn blur3(point: vec2<i32>, vertical: bool) -> vec4<f32> {
    var value = 0.0;
    for (var offset = -i32(glow3.radius); offset <= i32(glow3.radius); offset += 1) {
        let sample_point = point + select(vec2<i32>(offset, 0), vec2<i32>(0, offset), vertical);
        if all(sample_point >= vec2<i32>(0)) && all(sample_point < vec2<i32>(glow3.size)) {
            let index = u32(abs(offset));
            let weight = glow3.weights[index / 4u][index % 4u];
            let sample_value = textureLoad(image3, sample_point, 0);
            var alpha = sample_value.a;
            if vertical { alpha = unpack_mask(sample_value); }
            value += weight * alpha;
        }
    }
    return pack_mask(value);
}

struct Masks {
    @location(0) mask0: vec4<f32>,
    @location(1) mask1: vec4<f32>,
    @location(2) mask2: vec4<f32>,
    @location(3) mask3: vec4<f32>,
}

@fragment
fn fs_horizontal(@builtin(position) position: vec4<f32>) -> Masks {
    let point = vec2<i32>(position.xy);
    return Masks(blur0(point, false), blur1(point, false), blur2(point, false), blur3(point, false));
}

@fragment
fn fs_vertical(@builtin(position) position: vec4<f32>) -> Masks {
    let point = vec2<i32>(position.xy);
    return Masks(blur0(point, true), blur1(point, true), blur2(point, true), blur3(point, true));
}
