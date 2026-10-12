struct Camera {
    view_projection: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;
struct Lighting {
    position_enabled: vec4<f32>,
    color_intensity: vec4<f32>,
};
@group(0) @binding(1) var<uniform> lighting: Lighting;
@group(0) @binding(2) var<uniform> boundary_metrics: vec4<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) world0: vec4<f32>,
    @location(3) world1: vec4<f32>,
    @location(4) world2: vec4<f32>,
    @location(5) world3: vec4<f32>,
    @location(6) normal0: vec4<f32>,
    @location(7) normal1: vec4<f32>,
    @location(8) normal2: vec4<f32>,
    @location(9) color: vec4<f32>,
};
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world_position: vec3<f32>,
    @location(2) world_normal: vec3<f32>,
    @location(3) point_lit: f32,
};
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    let world = mat4x4<f32>(input.world0, input.world1, input.world2, input.world3);
    let normal_matrix = mat3x3<f32>(input.normal0.xyz, input.normal1.xyz, input.normal2.xyz);
    let world_position = world * vec4<f32>(input.position, 1.0);
    var output: VertexOutput;
    // Keep the established unlit clip-space evaluation order bit-for-bit.
    output.position = camera.view_projection * world * vec4<f32>(input.position, 1.0);
    output.color = input.color;
    output.world_position = world_position.xyz;
    output.world_normal = normal_matrix * input.normal;
    output.point_lit = input.normal0.w;
    return output;
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var result = input.color;
    if input.point_lit > 0.5 && lighting.position_enabled.w > 0.5 {
        let normal = stable_normalize(input.world_normal);
        // Scale the positions before subtraction as well, so opposite large
        // finite positions cannot overflow while computing the direction.
        let position_scale = max(
            max(max(abs(lighting.position_enabled.x), abs(lighting.position_enabled.y)), abs(lighting.position_enabled.z)),
            max(max(abs(input.world_position.x), abs(input.world_position.y)), abs(input.world_position.z)),
        );
        var toward_light = vec3<f32>(0.0, 0.0, 0.0);
        if position_scale > 0.0 && position_scale <= MAX_FINITE_F32 {
            toward_light = lighting.position_enabled.xyz / position_scale - input.world_position / position_scale;
        }
        let direction = stable_normalize(toward_light);
        if normal.w > 0.5 && direction.w > 0.5 {
            let cosine = dot(normal.xyz, direction.xyz);
            let cubic = cosine * cosine * cosine;
            var response = 0.25 * cubic;
            if cosine >= 0.0 {
                response = 0.5 * cubic;
            }
            result = vec4<f32>(result.rgb + lighting.color_intensity.rgb * lighting.color_intensity.a * response, result.a);
        }
    }
    return result;
}

struct EdgeInput {
    @location(0) start: vec3<f32>,
    @location(1) end: vec3<f32>,
    @location(10) corner: vec2<f32>,
    @location(11) face_point: vec3<f32>,
    @location(2) world0: vec4<f32>,
    @location(3) world1: vec4<f32>,
    @location(4) world2: vec4<f32>,
    @location(5) world3: vec4<f32>,
    @location(6) normal0: vec4<f32>,
    @location(7) normal1: vec4<f32>,
    @location(8) normal2: vec4<f32>,
    @location(9) color: vec4<f32>,
};
struct EdgeOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) @interpolate(flat) line: vec4<f32>,
};
struct BoundaryGeometry {
    position: vec4<f32>,
    line: vec4<f32>,
};
fn boundary_geometry(input: EdgeInput) -> BoundaryGeometry {
    let world = mat4x4<f32>(input.world0, input.world1, input.world2, input.world3);
    var a = camera.view_projection * world * vec4<f32>(input.start, 1.0);
    var b = camera.view_projection * world * vec4<f32>(input.end, 1.0);
    // Clip the centerline before dividing by W. Fully clipped segments collapse
    // instead of creating infinities during billboard extrusion.
    if a.z < 0.0 && b.z < 0.0 {
        a = vec4<f32>(0.0, 0.0, -1.0, 1.0);
        b = a;
    } else {
        if a.z < 0.0 { a = mix(a, b, a.z / (a.z - b.z)); }
        if b.z < 0.0 { b = mix(b, a, b.z / (b.z - a.z)); }
    }
    let viewport = boundary_metrics.xy;
    let delta = (b.xy / max(b.w, 1e-8) - a.xy / max(a.w, 1e-8))
        * (viewport / max(viewport.x, viewport.y));
    let scale = max(abs(delta.x), abs(delta.y));
    var perpendicular = vec2<f32>(0.0, 0.0);
    if scale > 1e-8 && scale <= MAX_FINITE_F32 {
        let direction = delta / scale;
        perpendicular = vec2<f32>(-direction.y, direction.x) / length(direction);
    }
    var p = mix(a, b, input.corner.x);
    // Retain the authored width in pixels. The support also covers the box
    // filter and MSAA sample offsets; alpha carries the actual pixel area.
    let half_width = input.normal1.w * 0.25
        * length(perpendicular * boundary_metrics.zw * viewport);
    p = vec4<f32>(p.xy + perpendicular * input.corner.y
        * (half_width + 1.5) * 2.0 / viewport * p.w, p.zw);
    // Screen extrusion must remain on its incident triangle's depth plane.
    // Centerline depth alone lets the adjacent fill occlude a subpixel border.
    let c = camera.view_projection * world * vec4<f32>(input.face_point, 1.0);
    if a.w > 1e-8 && b.w > 1e-8 && c.w > 1e-8 {
        let ab = b.xyz / b.w - a.xyz / a.w;
        let ac = c.xyz / c.w - a.xyz / a.w;
        let determinant = ab.x * ac.y - ab.y * ac.x;
        if abs(determinant) > 1e-12 && abs(determinant) <= MAX_FINITE_F32 {
            let depth_gradient = vec2<f32>(ab.z * ac.y - ac.z * ab.y,
                ab.x * ac.z - ac.x * ab.z) / determinant;
            if all(abs(depth_gradient) <= vec2<f32>(MAX_FINITE_F32)) {
                let centerline = mix(a, b, input.corner.x);
                p.z += dot(p.xy / p.w - centerline.xy / centerline.w, depth_gradient) * p.w;
            }
        }
    }
    let normal = perpendicular * vec2<f32>(1.0, -1.0);
    let start_pixel = (a.xy / max(a.w, 1e-8) * vec2<f32>(1.0, -1.0)
        + vec2<f32>(1.0)) * viewport * 0.5;
    var result: BoundaryGeometry;
    result.position = p;
    result.line = vec4<f32>(normal, dot(start_pixel, normal), half_width);
    return result;
}
fn pixel_normal_cdf(distance: f32, normal: vec2<f32>) -> f32 {
    // A square pixel projected onto an oblique normal has a trapezoidal
    // density. Integrating it avoids orientation-dependent thin-line energy.
    let major = max(abs(normal.x), abs(normal.y));
    let minor = min(abs(normal.x), abs(normal.y));
    if major <= 1e-8 { return 0.0; }
    if minor <= 1e-4 { return clamp(distance / major + 0.5, 0.0, 1.0); }
    let support = (major + minor) * 0.5;
    if distance <= -support { return 0.0; }
    if distance >= support { return 1.0; }
    let plateau = (major - minor) * 0.5;
    if distance < -plateau {
        let tail = distance + support;
        return tail * tail / (2.0 * major * minor);
    }
    if distance > plateau {
        let tail = support - distance;
        return 1.0 - tail * tail / (2.0 * major * minor);
    }
    return 0.5 + distance / major;
}
fn boundary_coverage(position: vec2<f32>, line: vec4<f32>) -> f32 {
    let distance = dot(position, line.xy) - line.z;
    return clamp(pixel_normal_cdf(distance + line.w, line.xy)
        - pixel_normal_cdf(distance - line.w, line.xy), 0.0, 1.0);
}
@vertex fn vs_boundary(input: EdgeInput) -> EdgeOutput {
    var result: EdgeOutput;
    let geometry = boundary_geometry(input);
    result.position = geometry.position;
    result.line = geometry.line;
    result.color = input.color;
    return result;
}
@fragment fn fs_boundary(input: EdgeOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color.rgb,
        input.color.a * boundary_coverage(input.position.xy, input.line));
}
