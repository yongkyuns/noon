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
    @location(14) stroke_metadata: vec3<f32>,
};

fn projected_tangent(world: mat4x4<f32>, tangent: vec2<f32>, clip: vec4<f32>, fixed: u32) -> vec2<f32> {
    if fixed != 0u {
        return (world * vec4<f32>(tangent, 0.0, 0.0)).xy;
    }
    let derivative = camera.view_projection * world * vec4<f32>(tangent, 0.0, 0.0);
    return (derivative.xy * clip.w - clip.xy * derivative.w) / fixed_camera.clip_scale;
}

fn stable_unit(value: vec2<f32>) -> vec2<f32> {
    let largest = max(abs(value.x), abs(value.y));
    if largest > 1e-8 && largest <= 3.402823e38 {
        return normalize(value / largest);
    }
    return vec2<f32>(0.0);
}
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) opacity: f32,
};

fn path_vertex(input: VertexInput, half_miter_limit: f32) -> VertexOutput {
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
    if input.stroke_metadata.x > 0.0 {
        let kind = input.stroke_metadata.x;
        let previous = stable_unit(projected_tangent(world, input.tangent, clip, input.fixed_orientation));
        let next = stable_unit(projected_tangent(world, input.extrusion, clip, input.fixed_orientation));
        let turn = previous.x * next.y - previous.y * next.x;
        if kind == 2.0 {
            // Fan/miter origins stay at the path centerline.
        } else if dot(previous, previous) > 0.5 && dot(next, next) > 0.5
            && (kind == 5.0 || abs(turn) > 1e-6) {
            let outer_sign = select(1.0, -1.0, turn > 0.0);
            let previous_normal = vec2<f32>(-previous.y, previous.x) * outer_sign;
            let next_normal = vec2<f32>(-next.y, next.x) * outer_sign;
            var offset = vec2<f32>(0.0);
            if kind == 1.0 {
                // The bevel edge is the straight chord between the two outer
                // half-width offsets, not a circular arc.
                offset = mix(previous_normal, next_normal, input.stroke_metadata.z) * 0.5;
            } else if kind == 3.0 {
                let miter = stable_unit(previous_normal + next_normal);
                let denominator = dot(miter, previous_normal);
                if denominator > 1e-6 && 0.5 / denominator <= half_miter_limit {
                    offset = miter * (0.5 / denominator);
                } else {
                    // The over-limit miter vertex collapses to the bevel edge.
                    offset = previous_normal * 0.5;
                }
            } else if kind == 4.0 {
                let angle = atan2(
                    previous_normal.x * next_normal.y - previous_normal.y * next_normal.x,
                    dot(previous_normal, next_normal),
                );
                let angle_at_vertex = angle * input.stroke_metadata.z;
                offset = vec2<f32>(
                    previous_normal.x * cos(angle_at_vertex) - previous_normal.y * sin(angle_at_vertex),
                    previous_normal.x * sin(angle_at_vertex) + previous_normal.y * cos(angle_at_vertex),
                ) * 0.5;
            } else if kind == 5.0 {
                let theta = 3.14159265 * input.stroke_metadata.z;
                let normal = vec2<f32>(-previous.y, previous.x);
                offset = (normal * cos(theta) + previous * (input.stroke_metadata.y * sin(theta))) * 0.5;
            }
            clip = vec4<f32>(
                clip.xy + offset * input.screen_stroke_width * fixed_camera.clip_scale * clip.w,
                clip.zw,
            );
        } else if kind != 2.0 {
            clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        }
    } else if any(input.tangent != vec2<f32>(0.0)) {
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

@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    return path_vertex(input, 2.0);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return cairo_source_color(input.color, input.opacity);
}
