// Signed edge contribution to a unit pixel's winding integral (Green's theorem).
// Clip Y first, then integrate the piecewise-linear clamp(X, 0, 1). Summing
// closed perimeter edges handles concavity and overlapping fan pieces without
// clipping a separate triangle for each piece. Horizontal edges contribute zero.
fn polygon_edge_pixel_area(p: vec2<f32>, q: vec2<f32>) -> f32 {
    let y = clamp(vec2<f32>(p.y, q.y), vec2<f32>(0.0), vec2<f32>(1.0));
    if y.x == y.y { return 0.0; }
    let t = (y - vec2<f32>(p.y)) / (q.y - p.y);
    let x = vec2<f32>(p.x) + (q.x - p.x) * t;
    let lower = min(x.x, x.y);
    let upper = max(x.x, x.y);
    var average = clamp(lower, 0.0, 1.0);
    if upper > lower {
        let inside = clamp(vec2<f32>(lower, upper), vec2<f32>(0.0), vec2<f32>(1.0));
        let ramp_area = (inside.y - inside.x) * (inside.y + inside.x) * 0.5;
        let saturated_width = max(0.0, upper - max(lower, 1.0));
        average = (ramp_area + saturated_width) / (upper - lower);
    }
    return (y.y - y.x) * average;
}

struct ClippedPolygon {
    // Keep the clipped vertices as fields rather than a dynamically indexed
    // array inside a function-space struct. FXC cannot materialize writes to
    // that array through a pointer, while scalar/vector fields remain
    // addressable and can be selected with this bounded switch.
    point0: vec2<f32>,
    point1: vec2<f32>,
    point2: vec2<f32>,
    point3: vec2<f32>,
    point4: vec2<f32>,
    point5: vec2<f32>,
    point6: vec2<f32>,
    point7: vec2<f32>,
    count: u32,
};

// The switches preserve a single bounded coverage kernel on DX12/FXC. Passing
// the polygon by pointer avoids copying all eight vertices for each edge read.
fn polygon_point(polygon: ptr<function, ClippedPolygon>, index: u32) -> vec2<f32> {
    switch index {
        case 0u: { return (*polygon).point0; }
        case 1u: { return (*polygon).point1; }
        case 2u: { return (*polygon).point2; }
        case 3u: { return (*polygon).point3; }
        case 4u: { return (*polygon).point4; }
        case 5u: { return (*polygon).point5; }
        case 6u: { return (*polygon).point6; }
        default: { return (*polygon).point7; }
    }
}

fn append_polygon_point(polygon: ptr<function, ClippedPolygon>, point: vec2<f32>) {
    switch (*polygon).count {
        case 0u: { (*polygon).point0 = point; }
        case 1u: { (*polygon).point1 = point; }
        case 2u: { (*polygon).point2 = point; }
        case 3u: { (*polygon).point3 = point; }
        case 4u: { (*polygon).point4 = point; }
        case 5u: { (*polygon).point5 = point; }
        case 6u: { (*polygon).point6 = point; }
        case 7u: { (*polygon).point7 = point; }
        default: { return; }
    }
    (*polygon).count += 1u;
}

const POLYGON_CLASSIFY_EPSILON: f32 = 0.0000019073486328125;

fn clip_polygon_axis(
    polygon: ptr<function, ClippedPolygon>,
    axis: u32,
    boundary: f32,
    keep_greater: bool,
) -> ClippedPolygon {
    var output: ClippedPolygon;
    output.count = 0u;
    if (*polygon).count == 0u {
        return output;
    }

    for (var index = 0u; index < 8u; index += 1u) {
        if index >= (*polygon).count {
            break;
        }
        let next = select(index + 1u, 0u, index + 1u == (*polygon).count);
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
fn classify_convex_edge(p: vec2<f32>, q: vec2<f32>, winding: f32) -> i32 {
    let edge = q - p;
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
        // Preserve the original classifier's immediate conservative fallback
        // when arithmetic is invalid, even if a later edge would be outside.
        return -2i;
    }
    if signed_centre < -support - guard {
        return 0i;
    }
    if signed_centre <= support + guard {
        return -1i;
    }
    return 1i;
}

fn classify_convex_pixel(polygon: ClippedPolygon) -> i32 {
    // This function is entered before clipping, with only the source triangle
    // and rectangle quad. Constant accesses and unrolled edges avoid the
    // switch-based dynamic access required by the later clipped polygon path.
    if polygon.count != 3u && polygon.count != 4u {
        return -1i;
    }
    let a = polygon.point0;
    let b = polygon.point1;
    let c = polygon.point2;
    let d = polygon.point3;
    let last = select(a, d, polygon.count == 4u);

    var twice_area = 0.0;
    var area_scale = 0.0;
    let positive0 = a.x * b.y;
    let negative0 = a.y * b.x;
    twice_area += positive0 - negative0;
    area_scale += abs(positive0) + abs(negative0);
    let positive1 = b.x * c.y;
    let negative1 = b.y * c.x;
    twice_area += positive1 - negative1;
    area_scale += abs(positive1) + abs(negative1);
    let positive2 = c.x * last.y;
    let negative2 = c.y * last.x;
    twice_area += positive2 - negative2;
    area_scale += abs(positive2) + abs(negative2);
    if polygon.count == 4u {
        let positive3 = d.x * a.y;
        let negative3 = d.y * a.x;
        twice_area += positive3 - negative3;
        area_scale += abs(positive3) + abs(negative3);
    }
    let area_guard = POLYGON_CLASSIFY_EPSILON * max(area_scale, 0.000000000001);
    if twice_area != twice_area || area_scale != area_scale || abs(twice_area) <= area_guard {
        return -1i;
    }
    let winding = select(-1.0, 1.0, twice_area > 0.0);
    var fully_inside = true;
    let edge0 = classify_convex_edge(a, b, winding);
    if edge0 == -2i {
        return -1i;
    }
    if edge0 == 0i {
        return 0i;
    }
    if edge0 < 0i {
        fully_inside = false;
    }
    let edge1 = classify_convex_edge(b, c, winding);
    if edge1 == -2i {
        return -1i;
    }
    if edge1 == 0i {
        return 0i;
    }
    if edge1 < 0i {
        fully_inside = false;
    }
    let edge2 = classify_convex_edge(c, last, winding);
    if edge2 == -2i {
        return -1i;
    }
    if edge2 == 0i {
        return 0i;
    }
    if edge2 < 0i {
        fully_inside = false;
    }
    if polygon.count == 4u {
        let edge3 = classify_convex_edge(d, a, winding);
        if edge3 == -2i {
            return -1i;
        }
        if edge3 == 0i {
            return 0i;
        }
        if edge3 < 0i {
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
    polygon.point0 = a;
    polygon.point1 = b;
    polygon.point2 = c;
    polygon.point3 = d;
    let classification = classify_convex_pixel(polygon);
    if classification == 0i {
        return 0.0;
    }
    if classification == 1i {
        return 1.0;
    }
    polygon = clip_polygon_axis(&polygon, 0u, 0.0, true);
    polygon = clip_polygon_axis(&polygon, 0u, 1.0, false);
    polygon = clip_polygon_axis(&polygon, 1u, 0.0, true);
    polygon = clip_polygon_axis(&polygon, 1u, 1.0, false);
    if polygon.count < 3u {
        return 0.0;
    }

    var twice_area = 0.0;
    for (var index = 0u; index < 8u; index += 1u) {
        if index >= polygon.count {
            break;
        }
        let next = select(index + 1u, 0u, index + 1u == polygon.count);
        let p = polygon_point(&polygon, index);
        let q = polygon_point(&polygon, next);
        twice_area += p.x * q.y - p.y * q.x;
    }
    return clamp(abs(twice_area) * 0.5, 0.0, 1.0);
}
