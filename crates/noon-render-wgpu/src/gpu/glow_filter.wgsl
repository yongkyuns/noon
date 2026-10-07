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
@group(0) @binding(0) var<uniform> glow: GlowUniform;
@group(0) @binding(1) var input_image: texture_2d<f32>;
@group(0) @binding(2) var blurred_mask: texture_2d<f32>;

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

fn in_capture(point: vec2<i32>) -> bool {
    return all(point >= vec2<i32>(0)) && all(point < vec2<i32>(glow.size));
}

fn kernel_weight(offset: i32) -> f32 {
    let index = u32(abs(offset));
    return glow.weights[index / 4u][index % 4u];
}

@fragment
fn fs_horizontal(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let point = vec2<i32>(position.xy);
    var value = 0.0;
    for (var offset = -i32(glow.radius); offset <= i32(glow.radius); offset += 1) {
        let sample_point = point + vec2<i32>(offset, 0);
        if in_capture(sample_point) {
            value += kernel_weight(offset) * textureLoad(input_image, sample_point, 0).a;
        }
    }
    // Missing texels are transparent zero. Never renormalize at an edge.
    return pack_mask(value);
}

@fragment
fn fs_vertical(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let point = vec2<i32>(position.xy);
    var value = 0.0;
    for (var offset = -i32(glow.radius); offset <= i32(glow.radius); offset += 1) {
        let sample_point = point + vec2<i32>(0, offset);
        if in_capture(sample_point) {
            value += kernel_weight(offset) * unpack_mask(textureLoad(input_image, sample_point, 0));
        }
    }
    return pack_mask(value);
}

@fragment
fn fs_composite(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let point = vec2<i32>(position.xy);
    let mask = unpack_mask(textureLoad(blurred_mask, point, 0));
    var source = textureLoad(input_image, point, 0);
    // Source preparation owns premultiplication. Zero-alpha RGB cannot leak.
    if source.a == 0.0 {
        source = vec4<f32>(0.0);
    }
    let halo_alpha = clamp(glow.control.x * glow.tint.a * mask, 0.0, 1.0);
    let halo = vec4<f32>(glow.tint.rgb * halo_alpha, halo_alpha);
    return glow.control.y * (source + (1.0 - source.a) * halo);
}
