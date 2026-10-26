struct Camera { center: vec2<f32>, clip_scale: vec2<f32>, viewport_size: vec2<f32>, viewport_origin: vec2<f32>, };
struct Image { translation: vec2<f32>, scale: vec2<f32>, rotation: f32, opacity: f32, sampling: u32, padding: u32, dimensions: vec2<f32>, crop_origin: vec2<f32>, };
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
    let center = (object.translation-camera.center)*camera.clip_scale
        *vec2(camera.viewport_size.x,-camera.viewport_size.y)*0.5
        +camera.viewport_size*0.5+camera.viewport_origin;
    let extent = vec2(abs(c)*half_size.x+abs(s)*half_size.y,
                      abs(s)*half_size.x+abs(c)*half_size.y)
        *camera.clip_scale*camera.viewport_size*0.5;
    // Match the integer destination crop in final output coordinates. Offscreen
    // capture origins can be negative; truncation must precede that translation.
    // Ordinary images and insets have a zero origin and retain the same crop.
    let crop_origin = object.crop_origin.xyxy;
    output.clip_bounds = trunc(vec4(center-extent,center+extent)+crop_origin)-crop_origin;
    // Flat camera values keep the existing vertex-only camera binding ABI.
    output.camera_pose = vec4(camera.center,camera.viewport_origin);
    output.camera_scale = vec4(camera.viewport_size,camera.clip_scale);
    return output;
}
fn pixel(point: vec2<i32>) -> vec4<f32> {
    // Captured insets retain spare texture capacity. Clamp to the active raster
    // rather than sampling its cleared padding at a filtered image boundary.
    let limits = vec2<i32>(object.dimensions)-vec2<i32>(1);
    let rgba = textureLoad(image, clamp(point, vec2<i32>(0), limits), 0);
    let alpha = rgba.a*object.opacity; return vec4(rgba.rgb*alpha, alpha);
}
fn cubic(value: f32) -> f32 { let x = abs(value); if x <= 1.0 { return (x-2.0)*x*x+1.0; } if x < 2.0 { return ((-x+5.0)*x-8.0)*x+4.0; } return 0.0; }
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
    let position = uv*object.dimensions-vec2(0.5); let base = vec2<i32>(floor(position)); let fraction = fract(position); var color: vec4<f32>;
    if object.sampling == 0u { color = pixel(vec2<i32>(floor(uv*object.dimensions))); }
    else if object.sampling == 1u { let top = mix(pixel(base), pixel(base+vec2(1,0)), fraction.x); let bottom = mix(pixel(base+vec2(0,1)), pixel(base+vec2(1,1)), fraction.x); color = mix(top, bottom, fraction.y); }
    else { color = vec4(0.0); for (var y = -1; y <= 2; y = y+1) { for (var x = -1; x <= 2; x = x+1) { color += pixel(base+vec2(x,y))*cubic(f32(x)-fraction.x)*cubic(f32(y)-fraction.y); } } color.a = clamp(color.a, 0.0, 1.0); color = vec4(clamp(color.rgb, vec3(0.0), vec3(color.a)), color.a); }
    return color;
}
