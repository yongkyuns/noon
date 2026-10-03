struct Camera {
    view_projection: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;
struct FixedCamera {
    clip_scale: vec2<f32>,
};
@group(1) @binding(0) var<uniform> fixed_camera: FixedCamera;

struct VertexInput {
    @location(0) local: vec2<f32>,
    @location(1) surface: u32,
    @location(2) world0: vec4<f32>,
    @location(3) world1: vec4<f32>,
    @location(4) world2: vec4<f32>,
    @location(5) world3: vec4<f32>,
    @location(6) fill: vec4<f32>,
    @location(7) stroke: vec4<f32>,
    @location(8) opacity: f32,
    @location(9) fixed_orientation: u32,
    @location(10) fixed_anchor: vec3<f32>,
};
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) opacity: f32,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    let world = mat4x4<f32>(input.world0, input.world1, input.world2, input.world3);
    let local_point = vec4<f32>(input.local, 0.0, 1.0);
    var clip = camera.view_projection * world * local_point;
    if input.fixed_orientation != 0u {
        // Project the object's effective center, then preserve its world-point
        // offsets in the 2D camera's clip scale. This is intentionally not a
        // camera-facing quaternion billboard.
        let center_clip = camera.view_projection * vec4<f32>(input.fixed_anchor, 1.0);
        let world_point = world * local_point;
        let world_offset = world_point.xyz - input.fixed_anchor;
        if center_clip.w > 0.0 {
            clip = vec4<f32>(
                center_clip.xy / center_clip.w + world_offset.xy * fixed_camera.clip_scale,
                center_clip.z / center_clip.w,
                1.0,
            );
        } else {
            clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        }
    }
    var output: VertexOutput;
    output.position = clip;
    output.color = select(input.fill, input.stroke, (input.surface & 1u) != 0u);
    output.opacity = input.opacity;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return cairo_source_color(input.color, input.opacity);
}
