struct Camera { center: vec2<f32>, clip_scale: vec2<f32>, viewport_size: vec2<f32>, viewport_origin: vec2<f32>, sample_center: vec2<f32>, sample_clip_scale: vec2<f32>, sample_viewport_size: vec2<f32>, sample_viewport_origin: vec2<f32>, };
struct Image { translation: vec2<f32>, scale: vec2<f32>, rotation: f32, opacity: f32, sampling: u32, emission: f32, dimensions: vec2<f32>, crop_origin: vec2<f32>, };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var image: texture_2d<f32>;
@group(2) @binding(0) var<uniform> object: Image;
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) rotation: vec2<f32>,
    @location(1) @interpolate(flat) clip_bounds: vec4<f32>,
    @location(2) @interpolate(flat) camera_pose: vec4<f32>,
    @location(3) @interpolate(flat) camera_scale: vec4<f32>,
};
@vertex fn vs_image(@builtin(vertex_index) index: u32) -> VertexOutput {
    let quad = array<vec2<f32>, 6>(vec2(-1.0,-1.0), vec2(1.0,-1.0), vec2(1.0,1.0), vec2(-1.0,-1.0), vec2(1.0,1.0), vec2(-1.0,1.0));
    let point = quad[index];
    let half_size = object.dimensions * 0.5 * abs(object.scale);
    let c = cos(object.rotation);
    let s = sin(object.rotation);
    // Bitmaps use pixel-center coverage, independent of the vector MSAA mode.
    // Cover every sample of an admitted pixel; the fragment clips the original
    // image footprint. Zero-scale images remain degenerate instead of dividing.
    let pixel_radius = vec2(1.0) / (camera.clip_scale * camera.viewport_size);
    let pad = vec2(abs(c)*pixel_radius.x + abs(s)*pixel_radius.y,
                   abs(s)*pixel_radius.x + abs(c)*pixel_radius.y);
    var expanded_half_size = half_size;
    if all(half_size > vec2(0.0)) {
        expanded_half_size += pad;
    }
    let local = point * expanded_half_size * sign(object.scale);
    let world = object.translation + vec2(c*local.x-s*local.y, s*local.x+c*local.y);
    var output: VertexOutput;
    output.position = vec4((world-camera.center)*camera.clip_scale, 0.0, 1.0);
    output.rotation = vec2(c,s);
    let center = (object.translation-camera.sample_center)*camera.sample_clip_scale
        *vec2(camera.sample_viewport_size.x,-camera.sample_viewport_size.y)*0.5
        +camera.sample_viewport_size*0.5+camera.sample_viewport_origin;
    let extent = vec2(abs(c)*half_size.x+abs(s)*half_size.y,
                      abs(s)*half_size.x+abs(c)*half_size.y)
        *camera.sample_clip_scale*camera.sample_viewport_size*0.5;
    // Match the integer destination crop in final output coordinates. Offscreen
    // capture origins can be negative; truncation must precede that translation.
    // Ordinary images and insets have a zero origin and retain the same crop.
    let crop_origin = object.crop_origin.xyxy;
    output.clip_bounds = trunc(vec4(center-extent,center+extent))-crop_origin;
    // Flat camera values keep the existing vertex-only camera binding ABI.
    output.camera_pose = vec4(camera.sample_center,camera.sample_viewport_origin-object.crop_origin);
    output.camera_scale = vec4(camera.sample_viewport_size,camera.sample_clip_scale);
    return output;
}
@fragment fn fs_image(input: VertexOutput) -> @location(0) vec4<f32> {
    let viewport_position = input.position.xy-input.camera_pose.zw;
    let world = (viewport_position/input.camera_scale.xy-vec2(0.5))
        *vec2(2.0,-2.0)/input.camera_scale.zw+input.camera_pose.xy;
    let delta = world-object.translation;
    let local = vec2(input.rotation.x*delta.x+input.rotation.y*delta.y,
                     -input.rotation.y*delta.x+input.rotation.x*delta.y);
    let normalized_local = local/(object.dimensions*object.scale)+vec2(0.5);
    let uv = vec2(normalized_local.x,1.0-normalized_local.y);
    if any(uv < vec2(0.0)) || any(uv >= vec2(1.0))
        || any(input.position.xy < input.clip_bounds.xy)
        || any(input.position.xy >= input.clip_bounds.zw) {
        discard;
    }
    let value = raster_sample(image, uv, vec2<u32>(object.dimensions), object.sampling, 1u, object.opacity);
    return vec4(value.rgb * (1.0 + object.emission), value.a);
}
