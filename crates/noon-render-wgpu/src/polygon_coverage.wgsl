struct ClippedPolygon {
    points: array<vec2<f32>, 8>,
    count: u32,
};

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

    var index = 0u;
    loop {
        if index >= polygon.count {
            break;
        }
        let next = select(index + 1u, 0u, index + 1u == polygon.count);
        let p = polygon.points[index];
        let q = polygon.points[next];
        let p_coordinate = select(p.y, p.x, axis == 0u);
        let q_coordinate = select(q.y, q.x, axis == 0u);
        let p_inside = select(p_coordinate <= boundary, p_coordinate >= boundary, keep_greater);
        let q_inside = select(q_coordinate <= boundary, q_coordinate >= boundary, keep_greater);

        // A convex quad clipped by four half-planes has at most eight vertices.
        if p_inside {
            output.points[output.count] = p;
            output.count += 1u;
        }
        if p_inside != q_inside {
            let t = (boundary - p_coordinate) / (q_coordinate - p_coordinate);
            output.points[output.count] = mix(p, q, t);
            output.count += 1u;
        }
        index += 1u;
    }
    return output;
}

// Returns 1 for a pixel wholly inside the convex polygon, 0 for one wholly
// outside, and -1 when an edge can touch it.
fn classify_convex_pixel(polygon: ClippedPolygon) -> i32 {
    var twice_area = 0.0;
    var area_scale = 0.0;
    var index = 0u;
    loop {
        if index >= polygon.count {
            break;
        }
        let next = select(index + 1u, 0u, index + 1u == polygon.count);
        let p = polygon.points[index];
        let q = polygon.points[next];
        let positive = p.x * q.y;
        let negative = p.y * q.x;
        twice_area += positive - negative;
        area_scale += abs(positive) + abs(negative);
        index += 1u;
    }
    let area_guard = POLYGON_CLASSIFY_EPSILON * max(area_scale, 0.000000000001);
    if twice_area != twice_area || area_scale != area_scale || abs(twice_area) <= area_guard {
        return -1i;
    }
    let winding = select(-1.0, 1.0, twice_area > 0.0);
    var fully_inside = true;
    index = 0u;
    loop {
        if index >= polygon.count {
            break;
        }
        let next = select(index + 1u, 0u, index + 1u == polygon.count);
        let p = polygon.points[index];
        let edge = polygon.points[next] - p;
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
        index += 1u;
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
    var index = 0u;
    loop {
        if index >= polygon.count {
            break;
        }
        let next = select(index + 1u, 0u, index + 1u == polygon.count);
        let p = polygon.points[index];
        let q = polygon.points[next];
        twice_area += p.x * q.y - p.y * q.x;
        index += 1u;
    }
    return clamp(abs(twice_area) * 0.5, 0.0, 1.0);
}
