// The standard torus is the tube-radius neighborhood of the unit XY circle.
// Center the ray at its closest point to the origin before solving its
// implicit quartic. This removes the cubic term and large camera translations.
fn torus_line(ray: ContourRay) -> ContourRay {
    let direction = normalize(ray.direction);
    return ContourRay(cross(direction, cross(ray.origin, direction)), direction);
}

fn torus_minima(line: ContourRay, radius: f32) -> vec2<f32> {
    // The derivative of the quartic, divided by four, is t³ + 3pt + 2q.
    let p = (dot(line.origin, line.origin) + 1.0 - radius * radius
        - 2.0 * dot(line.direction.xy, line.direction.xy)) / 3.0;
    let q = -dot(line.origin.xy, line.direction.xy);
    let discriminant = q * q + p * p * p;
    if discriminant <= 0.0 && p < 0.0 {
        let scale = sqrt(-p);
        let angle = acos(clamp(-q / (scale * scale * scale), -1.0, 1.0)) / 3.0;
        return 2.0 * scale * vec2<f32>(cos(angle + 2.0943951024), cos(angle));
    }
    if q == 0.0 {
        return vec2<f32>(0.0);
    }
    let term = pow(abs(q) + sqrt(max(discriminant, 0.0)), 1.0 / 3.0);
    let root = sign(-q) * (term - p / term);
    return vec2<f32>(root);
}

fn torus_contains(point: vec3<f32>, radius: f32) -> bool {
    let tube = vec2<f32>(length(point.xy) - 1.0, point.z);
    return dot(tube, tube) <= radius * radius;
}

fn torus_ray_hits(ray: ContourRay, perspective: bool, radius: f32) -> bool {
    let line = torus_line(ray);
    let bound = 1.0 + radius;
    if dot(line.origin, line.origin) > bound * bound {
        return false;
    }
    let roots = torus_minima(line, radius);
    // The even part of the quartic is unchanged by reflection. Its global
    // minimum is on the side opposite the linear term.
    let t = select(roots.x, roots.y, dot(line.origin.xy, line.direction.xy) > 0.0);
    if perspective && t < dot(ray.origin, line.direction) {
        return false;
    }
    return torus_contains(line.origin + t * line.direction, radius);
}

fn torus_ring_distance(view: ContourView, world: mat4x4<f32>, axis: vec2<f32>,
        center: vec2<f32>, ray: ContourRay, growth: vec3<f32>) -> f32 {
    // The bearing and center ray define a plane through the torus center.
    // Its intersection with the torus plane supplies a point on the centerline
    // guaranteed to be inside the projected tube.
    let plane = cross(ray.direction, growth);
    let radial = normalize(vec2<f32>(plane.y, -plane.x));
    let offset = camera.view_projection * world * vec4<f32>(radial, 0.0, 0.0);
    let a = view.center_clip + offset;
    let b = view.center_clip - offset;
    return max(dot(a.xy / a.w - center, axis), dot(b.xy / b.w - center, axis));
}

fn torus_point(view: ContourView, world: mat4x4<f32>, local: vec2<f32>,
        radius: f32, inner: bool, hole_open: bool) -> ProjectedContourPoint {
    let center = view.center_clip.xy / view.center_clip.w;
    let axis = normalize(local * fixed_camera.clip_scale);
    let ray = contour_ray(view, center);
    let growth = view.to_local * (view.axis_x * axis.x + view.axis_y * axis.y);
    let origin_growth = select(growth, vec3<f32>(0.0), view.perspective);
    let direction_growth = select(vec3<f32>(0.0), growth, view.perspective);
    var ink = 0.0;
    if hole_open {
        ink = torus_ring_distance(view, world, axis, center, ray, growth);
    }
    var empty = select(max(length(center) + 2.0, ink * 2.0), 0.0, inner);
    for (var i = 0u; i < 13u; i++) {
        let middle = (empty + ink) * 0.5;
        let candidate = ContourRay(ray.origin + origin_growth * middle,
            ray.direction + direction_growth * middle);
        if torus_ray_hits(candidate, view.perspective, radius) {
            ink = middle;
        } else {
            empty = middle;
        }
    }
    let edge = axis * ink;
    return ProjectedContourPoint(center + edge * length(local), center + edge);
}

fn torus_nearest(ray: ContourRay, perspective: bool, radius: f32) -> f32 {
    let line = torus_line(ray);
    let bound = 1.0 + radius;
    let extent_squared = bound * bound - dot(line.origin, line.origin);
    if extent_squared < 0.0 {
        return RAY_LIMIT;
    }
    let end = sqrt(extent_squared);
    var start = -end;
    if perspective {
        start = max(start, dot(ray.origin, line.direction));
        if start > end {
            return RAY_LIMIT;
        }
    }
    let roots = torus_minima(line, radius);
    var inside = clamp(roots.x, start, end);
    if !torus_contains(line.origin + inside * line.direction, radius) {
        inside = clamp(roots.y, start, end);
        if !torus_contains(line.origin + inside * line.direction, radius) {
            return RAY_LIMIT;
        }
    }
    // Search the first occupied interval so a rear tube cannot pull the ink
    // in front of a nearer tube or a separate opaque draw.
    for (var i = 0u; i < 12u; i++) {
        let middle = (start + inside) * 0.5;
        if torus_contains(line.origin + middle * line.direction, radius) {
            inside = middle;
        } else {
            start = middle;
        }
    }
    return inside;
}

fn torus_stroke_depth(view: ContourView, world: mat4x4<f32>, xy: vec2<f32>,
        boundary: vec2<f32>, radius: f32) -> f32 {
    // Keep the depth envelope a ring even when the authored tube nearly
    // reaches the axis.
    let depth_radius = min(radius * 1.01, (1.0 + radius) * 0.5);
    var ray = contour_ray(view, xy);
    var first = torus_nearest(ray, view.perspective, depth_radius);
    if first == RAY_LIMIT {
        ray = contour_ray(view, boundary);
        first = torus_nearest(ray, view.perspective, depth_radius);
    }
    if first == RAY_LIMIT {
        return view.center_clip.z / view.center_clip.w;
    }
    let line = torus_line(ray);
    let hit = camera.view_projection * world * vec4<f32>(line.origin + first * line.direction, 1.0);
    return hit.z / hit.w;
}
