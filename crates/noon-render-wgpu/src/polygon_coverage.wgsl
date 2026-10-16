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

// Closed triangle/quad perimeter integral. This computes the same pixel-box
// intersection area for fills and stroke pieces without growing a clipped
// polygon in every fragment. Coverage, not sample count or resolution, changes
// implementation; the encoded color, opacity and painter order stay unchanged.
fn polygon_perimeter_pixel_coverage(
    a: vec2<f32>, b: vec2<f32>, c: vec2<f32>, d: vec2<f32>, count: u32,
) -> f32 {
    var area = polygon_edge_pixel_area(a, b) + polygon_edge_pixel_area(b, c);
    if count == 4u {
        area += polygon_edge_pixel_area(c, d) + polygon_edge_pixel_area(d, a);
    } else {
        area += polygon_edge_pixel_area(c, a);
    }
    return clamp(abs(area), 0.0, 1.0);
}

// Only the original convex triangle/quad is classified. Boundary pixels use
// the integral above, so no eight-vertex scratch polygon or dynamic indexing
// is carried in the fill fragment shader (including its large interior).
struct CoveragePolygon {
    point0: vec2<f32>,
    point1: vec2<f32>,
    point2: vec2<f32>,
    point3: vec2<f32>,
    count: u32,
};

const POLYGON_CLASSIFY_EPSILON: f32 = 0.0000019073486328125;

// Returns 1 for a pixel wholly inside this half-plane, 0 for wholly outside,
// -1 for an edge pixel, and -2 for an arithmetic ambiguity.
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
        return -2i;
    }
    if signed_centre < -support - guard { return 0i; }
    if signed_centre <= support + guard { return -1i; }
    return 1i;
}

fn classify_convex_pixel(polygon: CoveragePolygon) -> i32 {
    if polygon.count != 3u && polygon.count != 4u { return -1i; }
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
    if twice_area != twice_area || area_scale != area_scale || abs(twice_area) <= area_guard { return -1i; }
    let winding = select(-1.0, 1.0, twice_area > 0.0);
    var fully_inside = true;
    let edge0 = classify_convex_edge(a, b, winding);
    if edge0 == -2i { return -1i; }
    if edge0 == 0i { return 0i; }
    if edge0 < 0i { fully_inside = false; }
    let edge1 = classify_convex_edge(b, c, winding);
    if edge1 == -2i { return -1i; }
    if edge1 == 0i { return 0i; }
    if edge1 < 0i { fully_inside = false; }
    let edge2 = classify_convex_edge(c, last, winding);
    if edge2 == -2i { return -1i; }
    if edge2 == 0i { return 0i; }
    if edge2 < 0i { fully_inside = false; }
    if polygon.count == 4u {
        let edge3 = classify_convex_edge(d, a, winding);
        if edge3 == -2i { return -1i; }
        if edge3 == 0i { return 0i; }
        if edge3 < 0i { fully_inside = false; }
    }
    return select(-1i, 1i, fully_inside);
}

fn polygon_pixel_coverage(
    a: vec2<f32>, b: vec2<f32>, c: vec2<f32>, d: vec2<f32>, count: u32,
) -> f32 {
    var polygon: CoveragePolygon;
    polygon.count = count;
    polygon.point0 = a;
    polygon.point1 = b;
    polygon.point2 = c;
    polygon.point3 = d;
    let classification = classify_convex_pixel(polygon);
    if classification == 0i { return 0.0; }
    if classification == 1i { return 1.0; }
    return polygon_perimeter_pixel_coverage(a, b, c, d, count);
}
