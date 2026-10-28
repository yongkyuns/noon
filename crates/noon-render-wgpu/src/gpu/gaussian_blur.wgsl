// Stateless encoded-premultiplied RGBA Gaussian. Three RGBA8 attachments retain
// 24 bits per component between passes without float render-target extensions.
struct BlurUniform {
    size: vec2<u32>,
    radius: u32,
    reserved: u32,
    control: vec4<f32>, // final scope opacity, reserved
    weights: array<vec4<f32>, 64>,
}
@group(0) @binding(0) var<uniform> blur: BlurUniform;
@group(0) @binding(1) var image_high: texture_2d<f32>;
@group(0) @binding(2) var image_middle: texture_2d<f32>;
@group(0) @binding(3) var image_low: texture_2d<f32>;

@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[index], 0.0, 1.0);
}

struct PackedColor {
    @location(0) high: vec4<f32>,
    @location(1) middle: vec4<f32>,
    @location(2) low: vec4<f32>,
}

fn pack_color(value: vec4<f32>) -> PackedColor {
    let bits = vec4<u32>(round(clamp(value, vec4<f32>(0.0), vec4<f32>(1.0)) * 16777215.0));
    var result: PackedColor;
    result.high = vec4<f32>((bits >> vec4<u32>(16u)) & vec4<u32>(255u)) / 255.0;
    result.middle = vec4<f32>((bits >> vec4<u32>(8u)) & vec4<u32>(255u)) / 255.0;
    result.low = vec4<f32>(bits & vec4<u32>(255u)) / 255.0;
    return result;
}

fn unpack_color(point: vec2<i32>) -> vec4<f32> {
    let high = round(textureLoad(image_high, point, 0) * 255.0);
    let middle = round(textureLoad(image_middle, point, 0) * 255.0);
    let low = round(textureLoad(image_low, point, 0) * 255.0);
    return (high * 65536.0 + middle * 256.0 + low) / 16777215.0;
}

fn outside(point: vec2<i32>) -> bool {
    return any(point < vec2<i32>(0)) || any(point >= vec2<i32>(blur.size));
}

fn coefficient(offset: i32) -> f32 {
    let at = u32(abs(offset));
    return blur.weights[at / 4u][at % 4u];
}

@fragment
fn fs_horizontal(@builtin(position) position: vec4<f32>) -> PackedColor {
    let point = vec2<i32>(position.xy);
    var color = vec4<f32>(0.0);
    let radius = i32(blur.radius);
    for (var offset = -radius; offset <= radius; offset += 1) {
        let sample_point = point + vec2<i32>(offset, 0);
        if !outside(sample_point) {
            color += textureLoad(image_high, sample_point, 0) * coefficient(offset);
        }
    }
    return pack_color(color);
}

@fragment
fn fs_vertical(@builtin(position) position: vec4<f32>) -> PackedColor {
    let point = vec2<i32>(position.xy);
    var color = vec4<f32>(0.0);
    let radius = i32(blur.radius);
    for (var offset = -radius; offset <= radius; offset += 1) {
        let sample_point = point + vec2<i32>(0, offset);
        if !outside(sample_point) {
            color += unpack_color(sample_point) * coefficient(offset);
        }
    }
    return pack_color(color);
}

@fragment
fn fs_output(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return unpack_color(vec2<i32>(position.xy)) * blur.control.x;
}
