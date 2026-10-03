struct ClippedPolygon {
    points: array<vec2<f32>, 8>,
    count: u32,
};

// FXC cannot address dynamically indexed arrays inside value structs. Keep
// constant element accesses so the same coverage kernel compiles on DX12.
fn polygon_point(polygon: ClippedPolygon, index: u32) -> vec2<f32> {
    switch index {
        case 0u: { return polygon.points[0]; }
        case 1u: { return polygon.points[1]; }
        case 2u: { return polygon.points[2]; }
        case 3u: { return polygon.points[3]; }
        case 4u: { return polygon.points[4]; }
        case 5u: { return polygon.points[5]; }
        case 6u: { return polygon.points[6]; }
        default: { return polygon.points[7]; }
    }
}

fn append_polygon_point(polygon: ptr<function, ClippedPolygon>, point: vec2<f32>) {
    switch (*polygon).count {
        case 0u: { (*polygon).points[0] = point; }
        case 1u: { (*polygon).points[1] = point; }
        case 2u: { (*polygon).points[2] = point; }
        case 3u: { (*polygon).points[3] = point; }
        case 4u: { (*polygon).points[4] = point; }
        case 5u: { (*polygon).points[5] = point; }
        case 6u: { (*polygon).points[6] = point; }
        case 7u: { (*polygon).points[7] = point; }
        default: { return; }
    }
    (*polygon).count += 1u;
}

const POLYGON_CLASSIFY_EPSILON: f32 = 0.0000019073486328125;

fn clip_polygon_axis(
    polygon: ClippedPolygon,
    axis: u32,
    boundary: f32,
    keep_greater: bool,
) -> ClippedPolygon {
    var output: ClippedPolygon;
    output.count = 0u;
    if polygon.count == 0u {
        return output;
    }

    for (var index = 0u; index < 8u; index += 1u) {
        if index >= polygon.count {
            break;
        }
        let next = select(index + 1u, 0u, index + 1u == polygon.count);
        let p = polygon_point(polygon, index);
        let q = polygon_point(polygon, next);
        let p_coordinate = select(p.y, p.x, axis == 0u);
        let q_coordinate = select(q.y, q.x, axis == 0u);
        let p_inside = select(p_coordinate <= boundary, p_coordinate >= boundary, keep_greater);
        let q_inside = select(q_coordinate <= boundary, q_coordinate >= boundary, keep_greater);

        // A convex quad clipped by four half-planes has at most eight vertices.
        if p_inside {
            append_polygon_point(&output, p);
        }
        if p_inside != q_inside {
            let t = (boundary - p_coordinate) / (q_coordinate - p_coordinate);
            append_polygon_point(&output, mix(p, q, t));
        }
    }
    return output;
}

// Returns 1 for a pixel wholly inside the convex polygon, 0 for one wholly
// outside, and -1 when an edge can touch it.
fn classify_convex_pixel(polygon: ClippedPolygon) -> i32 {
    var twice_area = 0.0;
    var area_scale = 0.0;
    for (var index = 0u; index < 8u; index += 1u) {
        if index >= polygon.count {
            break;
        }
        let next = select(index + 1u, 0u, index + 1u == polygon.count);
        let p = polygon_point(polygon, index);
        let q = polygon_point(polygon, next);
        let positive = p.x * q.y;
        let negative = p.y * q.x;
        twice_area += positive - negative;
        area_scale += abs(positive) + abs(negative);
    }
    let area_guard = POLYGON_CLASSIFY_EPSILON * max(area_scale, 0.000000000001);
    if twice_area != twice_area || area_scale != area_scale || abs(twice_area) <= area_guard {
        return -1i;
    }
    let winding = select(-1.0, 1.0, twice_area > 0.0);
    var fully_inside = true;
    for (var index = 0u; index < 8u; index += 1u) {
        if index >= polygon.count {
            break;
        }
        let next = select(index + 1u, 0u, index + 1u == polygon.count);
        let p = polygon_point(polygon, index);
        let edge = polygon_point(polygon, next) - p;
        let delta = vec2<f32>(0.5) - p;
        let positive = edge.x * delta.y;
        let negative = edge.y * delta.x;
        let signed_centre = winding * (positive - negative);
        let support = 0.5 * (abs(edge.x) + abs(edge.y));
        let guard = POLYGON_CLASSIFY_EPSILON * max(
            abs(positive) + abs(negative) + support,
            0.000000000001,
        );
        if signed_centre != signed_centre || support != support || guard != guard {
            return -1i;
        }
        if signed_centre < -support - guard {
            return 0i;
        }
        if signed_centre <= support + guard {
            fully_inside = false;
        }
    }
    return select(-1i, 1i, fully_inside);
}

fn polygon_pixel_coverage(
    a: vec2<f32>,
    b: vec2<f32>,
    c: vec2<f32>,
    d: vec2<f32>,
    count: u32,
) -> f32 {
    var polygon: ClippedPolygon;
    polygon.count = count;
    polygon.points[0] = a;
    polygon.points[1] = b;
    polygon.points[2] = c;
    polygon.points[3] = d;
    let classification = classify_convex_pixel(polygon);
    if classification == 0i {
        return 0.0;
    }
    if classification == 1i {
        return 1.0;
    }
    polygon = clip_polygon_axis(polygon, 0u, 0.0, true);
    polygon = clip_polygon_axis(polygon, 0u, 1.0, false);
    polygon = clip_polygon_axis(polygon, 1u, 0.0, true);
    polygon = clip_polygon_axis(polygon, 1u, 1.0, false);
    if polygon.count < 3u {
        return 0.0;
    }

    var twice_area = 0.0;
    for (var index = 0u; index < 8u; index += 1u) {
        if index >= polygon.count {
            break;
        }
        let next = select(index + 1u, 0u, index + 1u == polygon.count);
        let p = polygon_point(polygon, index);
        let q = polygon_point(polygon, next);
        twice_area += p.x * q.y - p.y * q.x;
    }
    return clamp(abs(twice_area) * 0.5, 0.0, 1.0);
}
