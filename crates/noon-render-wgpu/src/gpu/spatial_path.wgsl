struct Camera {
    view_projection: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;
struct FixedCamera {
    clip_scale: vec2<f32>,
    _padding: vec2<f32>,
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
    @location(11) tangent: vec2<f32>,
    @location(12) extrusion: vec2<f32>,
    @location(13) screen_stroke_width: f32,
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
    if any(input.tangent != vec2<f32>(0.0)) {
        // Project the retained centerline, then expand its stroke in the
        // authoring frame. The homogeneous derivative is the exact projected
        // straight-line direction, including perspective and world rotation.
        var direction = input.tangent;
        if input.fixed_orientation == 0u {
            let derivative = camera.view_projection * world * vec4<f32>(input.tangent, 0.0, 0.0);
            direction = (derivative.xy * clip.w - clip.xy * derivative.w) / fixed_camera.clip_scale;
        } else {
            direction = (world * vec4<f32>(input.tangent, 0.0, 0.0)).xy;
        }
        let largest = max(abs(direction.x), abs(direction.y));
        if largest > 1e-8 && largest <= 3.402823e38 {
            let unit = normalize(direction / largest);
            let normal = vec2<f32>(-unit.y, unit.x);
            let offset = (unit * input.extrusion.x + normal * input.extrusion.y) * input.screen_stroke_width;
            clip = vec4<f32>(clip.xy + offset * fixed_camera.clip_scale * clip.w, clip.zw);
        } else {
            // An end-on segment has no projected centerline. Collapse it
            // coherently instead of normalizing a zero/non-finite direction.
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
