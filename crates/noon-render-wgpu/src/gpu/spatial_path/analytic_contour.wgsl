// Derived view data shared by retained analytic silhouettes. The perspective
// eye is the point whose clip X, Y and W are zero.
struct ContourView {
    to_local: mat3x3<f32>,
    axis_x: vec3<f32>,
    axis_y: vec3<f32>,
    forward: vec3<f32>,
    eye_local: vec3<f32>,
    center_clip: vec4<f32>,
    perspective: bool,
    valid: bool,
};

struct ContourRay {
    origin: vec3<f32>,
    direction: vec3<f32>,
};

fn projection_row(index: u32) -> vec3<f32> {
    let p = camera.view_projection;
    return vec3<f32>(p[0][index], p[1][index], p[2][index]);
}

fn contour_view(world: mat4x4<f32>) -> ContourView {
    var view: ContourView;
    let x = world[0].xyz;
    let y = world[1].xyz;
    let z = world[2].xyz;
    let scale_squared = vec3<f32>(dot(x, x), dot(y, y), dot(z, z));
    if min(min(scale_squared.x, scale_squared.y), scale_squared.z) <= 1e-20 {
        return view;
    }
    // A semantic world transform is rotation times per-axis scale.
    view.to_local = transpose(mat3x3<f32>(
        x / scale_squared.x, y / scale_squared.y, z / scale_squared.z,
    ));
    let view_x = projection_row(0u);
    let view_y = projection_row(1u);
    view.axis_x = view_x / dot(view_x, view_x);
    view.axis_y = view_y / dot(view_y, view_y);
    view.forward = projection_row(3u);
    view.perspective = dot(view.forward, view.forward) > 0.0;
    view.center_clip = camera.view_projection * world[3];
    let center = view.center_clip;
    if view.perspective {
        view.eye_local = view.to_local * -(view.axis_x * center.x
            + view.axis_y * center.y + view.forward * center.w);
    } else {
        view.forward = normalize(projection_row(2u));
    }
    view.valid = center.w > 0.0;
    return view;
}

fn contour_ray(view: ContourView, xy: vec2<f32>) -> ContourRay {
    if view.perspective {
        return ContourRay(view.eye_local,
            view.to_local * (view.forward + view.axis_x * xy.x + view.axis_y * xy.y));
    }
    return ContourRay(view.to_local * (view.axis_x * (xy.x - view.center_clip.x)
        + view.axis_y * (xy.y - view.center_clip.y)), view.to_local * view.forward);
}

fn sphere_plane(view: ContourView, world: mat4x4<f32>) -> mat4x4<f32> {
    var toward = vec3<f32>(0.0);
    var tangent_offset = 0.0;
    if view.perspective {
        let distance = length(view.eye_local);
        if distance <= 1.0 {
            return mat4x4<f32>();
        }
        toward = view.eye_local / distance;
        tangent_offset = 1.0 / distance;
    } else {
        toward = normalize(view.to_local * -view.forward);
    }
    let across = cross(view.to_local * projection_row(1u), toward);
    if !view.valid || dot(across, across) <= 1e-20 {
        return mat4x4<f32>();
    }
    // Camera up fixes the phase independently of the fill mesh.
    let radius = sqrt(1.0 - tangent_offset * tangent_offset);
    let right = normalize(across) * radius;
    let up = cross(toward, right);
    return world * mat4x4<f32>(
        vec4<f32>(right, 0.0), vec4<f32>(up, 0.0),
        vec4<f32>(toward, 0.0), vec4<f32>(toward * tangent_offset, 1.0),
    );
}

fn sphere_stroke_depth(view: ContourView, world: mat4x4<f32>, clip: vec4<f32>) -> f32 {
    let ray = contour_ray(view, clip.xy / clip.w);
    let direction = normalize(ray.direction);
    let closest = cross(direction, cross(ray.origin, direction));
    // Use the front surface depth with a half-percent radial margin. Near
    // tangency this keeps the linearly interpolated ribbon above the fill.
    let front = sqrt(max(1.01 - dot(closest, closest), 0.0));
    let nearest = camera.view_projection * world * vec4<f32>(closest - direction * front, 1.0);
    return nearest.z / nearest.w * clip.w;
}

const RAY_LIMIT: f32 = 1e20;

fn axial_interval(ray: ContourRay, perspective: bool, half_height: f32) -> vec2<f32> {
    let first = select(-RAY_LIMIT, 0.0, perspective);
    if abs(ray.direction.z) <= 1e-8 {
        return select(vec2<f32>(1.0, -1.0), vec2<f32>(first, RAY_LIMIT),
            abs(ray.origin.z) <= half_height);
    }
    let a = (-half_height - ray.origin.z) / ray.direction.z;
    let b = (half_height - ray.origin.z) / ray.direction.z;
    return vec2<f32>(max(first, min(a, b)), max(a, b));
}

fn cylinder_ray_hits(ray: ContourRay, perspective: bool) -> bool {
    let axial = axial_interval(ray, perspective, 1.0);
    if axial.x > axial.y {
        return false;
    }
    // For this visibility query only the closest radial approach inside the
    // axial slab matters. No quadratic roots are needed during the search.
    let axis = ray.direction.xy;
    let nearest = clamp(-dot(ray.origin.xy, axis) / max(dot(axis, axis), 1e-20), axial.x, axial.y);
    let point = ray.origin.xy + nearest * axis;
    return dot(point, point) <= 1.0;
}

// A quadratic tests which points along a ray lie inside the cone side:
// |origin.xy + t * direction.xy|² - ((tip - origin.z - t * direction.z) / 2)².
fn cone_side(point: vec3<f32>, tip: f32) -> f32 {
    let radius = (tip - point.z) * 0.5;
    return dot(point.xy, point.xy) - radius * radius;
}

fn cone_ray_hits(ray: ContourRay, perspective: bool) -> bool {
    let axial = axial_interval(ray, perspective, 1.0);
    if axial.x > axial.y {
        return false;
    }
    let slope = -ray.direction.z * 0.5;
    let a = dot(ray.direction.xy, ray.direction.xy) - slope * slope;
    if a > 0.0 {
        let b = dot(ray.origin.xy, ray.direction.xy) - (1.0 - ray.origin.z) * 0.5 * slope;
        let t = clamp(-b / a, axial.x, axial.y);
        return cone_side(ray.origin + t * ray.direction, 1.0) <= 0.0;
    }
    // A linear or concave quadratic takes its minimum at a slab endpoint.
    return cone_side(ray.origin + axial.x * ray.direction, 1.0) <= 0.0
        || cone_side(ray.origin + axial.y * ray.direction, 1.0) <= 0.0;
}

fn axial_ray_hits(ray: ContourRay, perspective: bool, cone: bool) -> bool {
    if cone {
        return cone_ray_hits(ray, perspective);
    }
    return cylinder_ray_hits(ray, perspective);
}

struct AxialPoint {
    xy: vec2<f32>,
    boundary: vec2<f32>,
};

fn axial_point(view: ContourView, local: vec2<f32>, cone: bool) -> AxialPoint {
    let center = view.center_clip.xy / view.center_clip.w;
    let image_axis = normalize(local * fixed_camera.clip_scale);
    let ray = contour_ray(view, center);
    let growth = view.to_local * (view.axis_x * image_axis.x + view.axis_y * image_axis.y);
    let origin_growth = select(growth, vec3<f32>(0.0), view.perspective);
    let direction_growth = select(vec3<f32>(0.0), growth, view.perspective);
    var inside = 0.0;
    // The volume is convex and the center ray hits. Search out beyond the
    // viewport; a contour offscreen stays offscreen when the camera is close.
    var outside = length(center) + 2.0;
    for (var i = 0u; i < 13u; i++) {
        let middle = (inside + outside) * 0.5;
        let candidate = ContourRay(ray.origin + origin_growth * middle,
            ray.direction + direction_growth * middle);
        if axial_ray_hits(candidate, view.perspective, cone) {
            inside = middle;
        } else {
            outside = middle;
        }
    }
    let edge = image_axis * inside;
    // Keeping the sketch in image-angle order gives the side and rim one
    // shared phase, including when a side disappears behind the near cap.
    return AxialPoint(center + edge * length(local), center + edge);
}

fn cylinder_nearest(ray: ContourRay, perspective: bool) -> f32 {
    // A one-percent depth envelope keeps the ribbon above the fill where
    // interpolated triangles cross the sharp cap/side boundary.
    let axial = axial_interval(ray, perspective, 1.01);
    let radius_squared = 1.0201;
    let axis = ray.direction.xy;
    let origin = ray.origin.xy;
    let speed_squared = dot(axis, axis);
    if speed_squared <= 1e-20 {
        return select(RAY_LIMIT, axial.x, axial.x <= axial.y && dot(origin, origin) <= radius_squared);
    }
    let lateral = axis.x * origin.y - axis.y * origin.x;
    let discriminant = speed_squared * radius_squared - lateral * lateral;
    if discriminant < 0.0 {
        return RAY_LIMIT;
    }
    let midpoint = -dot(origin, axis) / speed_squared;
    let radius = sqrt(discriminant) / speed_squared;
    let first = max(axial.x, midpoint - radius);
    return select(RAY_LIMIT, first, first <= min(axial.y, midpoint + radius));
}

fn cone_nearest(ray: ContourRay, perspective: bool) -> f32 {
    // Use the same small depth envelope as the capped cylinder.
    let tip = 1.01;
    let axial = axial_interval(ray, perspective, tip);
    if axial.x > axial.y {
        return RAY_LIMIT;
    }
    if axial.x > -RAY_LIMIT && cone_side(ray.origin + axial.x * ray.direction, tip) <= 0.0 {
        return axial.x;
    }
    let d = ray.direction;
    let o = ray.origin;
    let radius = (tip - o.z) * 0.5;
    let slope = -d.z * 0.5;
    let a = dot(d.xy, d.xy) - slope * slope;
    let b = dot(o.xy, d.xy) - radius * slope;
    let c = cone_side(o, tip);
    var first = RAY_LIMIT;
    if a == 0.0 {
        if b < 0.0 {
            first = -c / (2.0 * b);
        }
    } else {
        // This form of b² - ac avoids subtracting large axial distances.
        let radial = radius * d.xy - slope * o.xy;
        let lateral = d.x * o.y - d.y * o.x;
        let discriminant = dot(radial, radial) - lateral * lateral;
        if discriminant >= 0.0 {
            let root = sqrt(discriminant);
            let q = -b - select(-root, root, b >= 0.0);
            if q != 0.0 {
                let a_root = q / a;
                let b_root = c / q;
                first = select(max(a_root, b_root), min(a_root, b_root), a > 0.0);
            } else {
                first = -b / a;
            }
        }
    }
    return select(RAY_LIMIT, first, first >= axial.x && first <= axial.y);
}

fn axial_nearest(ray: ContourRay, perspective: bool, cone: bool) -> f32 {
    if cone {
        return cone_nearest(ray, perspective);
    }
    return cylinder_nearest(ray, perspective);
}

fn axial_stroke_depth(view: ContourView, world: mat4x4<f32>,
        xy: vec2<f32>, boundary: vec2<f32>, cone: bool) -> f32 {
    var ray = contour_ray(view, xy);
    var first = axial_nearest(ray, view.perspective, cone);
    if first == RAY_LIMIT {
        // Ink just outside the solid keeps the depth of the nearby contour.
        ray = contour_ray(view, boundary);
        first = axial_nearest(ray, view.perspective, cone);
    }
    if first == RAY_LIMIT {
        return view.center_clip.z / view.center_clip.w;
    }
    let hit = camera.view_projection * world * vec4<f32>(ray.origin + first * ray.direction, 1.0);
    return hit.z / hit.w;
}
