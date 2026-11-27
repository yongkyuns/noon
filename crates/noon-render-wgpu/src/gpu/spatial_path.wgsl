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
    @location(9) reveal_and_flags: u32,
    @location(10) fixed_anchor: vec3<f32>,
    @location(11) tangent: vec2<f32>,
    @location(12) extrusion: vec2<f32>,
    @location(13) screen_stroke_width: f32,
    @location(14) stroke_metadata: vec3<f32>,
    @location(15) background_stroke: vec4<f32>,
};
const PATH_BACKGROUND_STROKE_FLAG: u32 = 0x80000000u;

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
    @location(2) path_progress: f32,
    @location(3) @interpolate(flat) reveal: f32,
};

// The camera rows are scaled world-space right, up and forward vectors. In
// perspective the eye is the unique point whose clip X, Y and W are zero.
struct SphereContour {
    plane: mat4x4<f32>,
    to_local: mat3x3<f32>,
    axis_x: vec3<f32>,
    axis_y: vec3<f32>,
    forward: vec3<f32>,
    eye_local: vec3<f32>,
    center_clip: vec4<f32>,
    perspective: bool,
    visible: bool,
};

fn projection_row(index: u32) -> vec3<f32> {
    let p = camera.view_projection;
    return vec3<f32>(p[0][index], p[1][index], p[2][index]);
}

fn sphere_contour(world: mat4x4<f32>) -> SphereContour {
    var contour: SphereContour;
    let x = world[0].xyz;
    let y = world[1].xyz;
    let z = world[2].xyz;
    let scale_squared = vec3<f32>(dot(x, x), dot(y, y), dot(z, z));
    if min(min(scale_squared.x, scale_squared.y), scale_squared.z) <= 1e-20 {
        return contour;
    }
    // A semantic world transform is rotation times per-axis scale.
    contour.to_local = transpose(mat3x3<f32>(
        x / scale_squared.x, y / scale_squared.y, z / scale_squared.z,
    ));
    let view_x = projection_row(0u);
    let view_y = projection_row(1u);
    contour.axis_x = view_x / dot(view_x, view_x);
    contour.axis_y = view_y / dot(view_y, view_y);
    contour.forward = projection_row(3u);
    contour.perspective = dot(contour.forward, contour.forward) > 0.0;
    contour.center_clip = camera.view_projection * world[3];
    let center = contour.center_clip;
    var toward = vec3<f32>(0.0);
    var tangent_offset = 0.0;
    if contour.perspective {
        contour.eye_local = contour.to_local * -(contour.axis_x * center.x
            + contour.axis_y * center.y + contour.forward * center.w);
        let distance = length(contour.eye_local);
        if distance <= 1.0 || center.w <= 0.0 {
            return contour;
        }
        toward = contour.eye_local / distance;
        tangent_offset = 1.0 / distance;
    } else {
        contour.forward = normalize(projection_row(2u));
        toward = normalize(contour.to_local * -contour.forward);
    }
    let across = cross(contour.to_local * view_y, toward);
    if dot(across, across) <= 1e-20 {
        return contour;
    }
    // The camera up vector fixes the phase of the retained sketch around
    // the tangent circle, independent of the sphere's triangle grid.
    let radius = sqrt(1.0 - tangent_offset * tangent_offset);
    let right = normalize(across) * radius;
    let up = cross(toward, right);
    contour.plane = world * mat4x4<f32>(
        vec4<f32>(right, 0.0), vec4<f32>(up, 0.0),
        vec4<f32>(toward, 0.0), vec4<f32>(toward * tangent_offset, 1.0),
    );
    contour.visible = true;
    return contour;
}

fn sphere_stroke_depth(contour: SphereContour, world: mat4x4<f32>, clip: vec4<f32>) -> f32 {
    let xy = clip.xy / clip.w;
    var direction = contour.to_local * contour.forward;
    var origin = contour.to_local * (contour.axis_x * (xy.x - contour.center_clip.x)
        + contour.axis_y * (xy.y - contour.center_clip.y));
    if contour.perspective {
        direction = contour.to_local
            * (contour.forward + contour.axis_x * xy.x + contour.axis_y * xy.y);
        origin = contour.eye_local;
    }
    direction = normalize(direction);
    let closest = cross(direction, cross(origin, direction));
    // Use the front surface depth with a half-percent radial margin. Near
    // tangency this keeps the linearly interpolated ribbon above the fill.
    let front = sqrt(max(1.01 - dot(closest, closest), 0.0));
    let nearest = camera.view_projection * world * vec4<f32>(closest - direction * front, 1.0);
    return nearest.z / nearest.w * clip.w;
}

fn path_vertex(input: VertexInput, half_miter_limit: f32) -> VertexOutput {
    let source_world = mat4x4<f32>(input.world0, input.world1, input.world2, input.world3);
    var world = source_world;
    let fixed_orientation = input.reveal_and_flags >> 31u;
    let sphere = (input.reveal_and_flags & 0x40000000u) != 0u;
    var contour: SphereContour;
    if sphere {
        contour = sphere_contour(source_world);
        world = contour.plane;
    }
    let local_point = vec4<f32>(input.local, 0.0, 1.0);
    var clip = camera.view_projection * world * local_point;
    if fixed_orientation != 0u {
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
        let previous = stable_unit(projected_tangent(world, input.tangent, clip, fixed_orientation));
        let next = stable_unit(projected_tangent(world, input.extrusion, clip, fixed_orientation));
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
        if fixed_orientation == 0u {
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
    if sphere {
        if contour.visible && clip.w > 0.0 {
            clip.z = sphere_stroke_depth(contour, source_world, clip);
        } else {
            clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        }
    }
    var output: VertexOutput;
    output.position = clip;
    output.color = select(input.fill, input.stroke, (input.surface & 1u) != 0u);
    if (input.surface & PATH_BACKGROUND_STROKE_FLAG) != 0u {
        output.color = input.background_stroke;
    }
    output.opacity = input.opacity;
    output.path_progress = f32((input.surface >> 1u) & 0x00ffffffu) / 16777215.0;
    output.reveal = bitcast<f32>(input.reveal_and_flags & 0x3fffffffu);
    return output;
}

@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    return path_vertex(input, 2.0);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Discard before writing depth so unrevealed geometry cannot occlude other objects.
    if input.reveal <= 0.0 || (input.reveal < 1.0 && input.path_progress > input.reveal) {
        discard;
    }
    return cairo_source_color(input.color, input.opacity);
}

@fragment
fn fs_fading_fill(input: VertexOutput) -> @location(0) vec4<f32> {
    let coverage = smoothstep(0.0, 1.0, input.reveal);
    return cairo_source_color(input.color, input.opacity * coverage);
}
