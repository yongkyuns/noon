struct Camera {
    center: vec2<f32>, clip_scale: vec2<f32>,
    viewport_size: vec2<f32>, padding: vec2<f32>,
};
struct Image {
    translation: vec2<f32>, scale: vec2<f32>,
    rotation: f32, opacity: f32, sampling: u32, padding: u32,
    dimensions: vec2<f32>, padding2: vec2<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(2) @binding(0) var<uniform> object: Image;
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex
fn vs_image(@builtin(vertex_index) index: u32) -> VertexOutput {
    let quad = array<vec2<f32>, 6>(vec2(-1.0,-1.0), vec2(1.0,-1.0), vec2(1.0,1.0), vec2(-1.0,-1.0), vec2(1.0,1.0), vec2(-1.0,1.0));
    let point = quad[index];
    let local = point * object.dimensions * 0.5 * object.scale;
    let c = cos(object.rotation);
    let s = sin(object.rotation);
    let world = object.translation + vec2(c*local.x-s*local.y, s*local.x+c*local.y);
    var output: VertexOutput;
    output.position = vec4((world-camera.center)*camera.clip_scale, 0.0, 1.0);
    // Canonical RGBA8 is top-row first; semantic coordinates are y-up.
    output.uv = vec2(point.x+1.0, 1.0-point.y)*0.5;
    return output;
}
fn pixel(point: vec2<i32>) -> vec4<f32> {
    let limits = vec2<i32>(textureDimensions(image, 0))-vec2<i32>(1);
    let rgba = textureLoad(image, clamp(point, vec2<i32>(0), limits), 0);
    // Interpolate premultiplied colors so transparent colored pixels do not halo.
    return vec4(rgba.rgb*rgba.a, rgba.a);
}
fn cubic(value: f32) -> f32 {
    let x = abs(value);
    if x <= 1.0 { return (1.5*x-2.5)*x*x+1.0; }
    if x < 2.0 { return ((-0.5*x+2.5)*x-4.0)*x+2.0; }
    return 0.0;
}
@fragment
fn fs_image(input: VertexOutput) -> @location(0) vec4<f32> {
    let position = input.uv*object.dimensions-vec2(0.5);
    let base = vec2<i32>(floor(position));
    let fraction = fract(position);
    var color: vec4<f32>;
    if object.sampling == 0u {
        color = pixel(vec2<i32>(floor(input.uv*object.dimensions)));
    } else if object.sampling == 1u {
        let top = mix(pixel(base), pixel(base+vec2(1,0)), fraction.x);
        let bottom = mix(pixel(base+vec2(0,1)), pixel(base+vec2(1,1)), fraction.x);
        color = mix(top, bottom, fraction.y);
    } else {
        color = vec4(0.0);
        for (var y = -1; y <= 2; y = y+1) {
            for (var x = -1; x <= 2; x = x+1) {
                color += pixel(base+vec2(x,y))*cubic(f32(x)-fraction.x)*cubic(f32(y)-fraction.y);
            }
        }
        color.a = clamp(color.a, 0.0, 1.0);
        color = vec4(clamp(color.rgb, vec3(0.0), vec3(color.a)), color.a);
    }
    return color*object.opacity;
}
