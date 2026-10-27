// All inputs are encoded-premultiplied RGBA on the same pixel grid.
// The caller supplies the processed painter prefix, target paint and original
// target coverage separately. No scene traversal, time or prefix selection here.
struct Placement {
    origin: vec2<i32>,
    size: vec2<u32>,
    viewport: vec2<u32>,
    opacity: f32,
    padding: u32,
}
@group(0) @binding(0) var<uniform> placement: Placement;
@group(0) @binding(1) var prefix: texture_2d<f32>;
@group(0) @binding(2) var paint: texture_2d<f32>;
@group(0) @binding(3) var coverage: texture_2d<f32>;

@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let vertices = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(vertices[index], 0.0, 1.0);
}
struct Composite {
    @location(0) contribution: vec4<f32>,
    @location(1) erase: vec4<f32>,
}
@fragment
fn fs_prepare(@builtin(position) pixel: vec4<f32>) -> Composite {
    let at = vec2<i32>(pixel.xy);
    let mask = textureLoad(coverage, at, 0).a;
    let original = textureLoad(paint, at, 0);
    // The target's paint is clipped to its original coverage even when an
    // upstream painted treatment spreads outside it. Preserve premultiplication.
    var clipped = original;
    if original.a > mask {
        clipped = original * (mask / original.a);
    }
    let background = textureLoad(prefix, at, 0);
    var result: Composite;
    result.contribution = (clipped + background * (mask - clipped.a)) * placement.opacity;
    result.erase = vec4<f32>(0.0, 0.0, 0.0, mask * placement.opacity);
    return result;
}
@vertex
fn vs_tile(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let pixel = vec2<f32>(placement.origin) + corners[index] * vec2<f32>(placement.size);
    let normalized = pixel / vec2<f32>(placement.viewport);
    return vec4<f32>(2.0 * normalized.x - 1.0, 1.0 - 2.0 * normalized.y, 0.0, 1.0);
}
@fragment
fn fs_tile(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    // Binding 1 is the prepared contribution or erase image for placement.
    // Scope opacity is already in these images and must not be applied twice.
    return textureLoad(prefix, vec2<i32>(pixel.xy) - placement.origin, 0);
}
