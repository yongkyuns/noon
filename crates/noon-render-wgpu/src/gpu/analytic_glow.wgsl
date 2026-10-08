// Place a fully composed local image at its existing primitive painter anchor.
// Texture coordinates are physical pixel coordinates, without filtering,
// resampling, shader time, or another output transfer.
struct Placement {
    origin: vec2<i32>,
    size: vec2<u32>,
    viewport: vec2<u32>,
    padding: vec2<u32>,
}
@group(0) @binding(0) var<uniform> placement: Placement;
@group(0) @binding(1) var image: texture_2d<f32>;

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
    return textureLoad(image, vec2<i32>(pixel.xy) - placement.origin, 0);
}
