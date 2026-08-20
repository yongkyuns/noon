from pathlib import Path

path = Path("crates/noon-geometry/src/tessellation.rs")
text = path.read_text()

old_const = "const PATH_TESSELLATION_TOLERANCE: f32 = 0.01;\n"
new_const = "const PATH_TESSELLATION_TOLERANCE: f32 = 0.01;\nconst MORPH_MITER_LIMIT: f32 = 4.0;\n"
if old_const not in text:
    raise SystemExit("tessellation tolerance marker missing")
text = text.replace(old_const, new_const, 1)

old = '''fn stroke_edges(points: &[Vec2], closed: bool, half_width: f32) -> Vec<(Vec2, Vec2)> {
    let mut result = Vec::with_capacity(points.len());
    for index in 0..points.len() {
        let previous = if index > 0 {
            points[index - 1]
        } else if closed {
            points[points.len() - 1]
        } else {
            points[index]
        };
        let next = if index + 1 < points.len() {
            points[index + 1]
        } else if closed {
            points[0]
        } else {
            points[index]
        };
        let tangent = normalized(Vec2::new(next.x - previous.x, next.y - previous.y));
        let normal = Vec2::new(-tangent.y * half_width, tangent.x * half_width);
        let point = points[index];
        result.push((
            Vec2::new(point.x + normal.x, point.y + normal.y),
            Vec2::new(point.x - normal.x, point.y - normal.y),
        ));
    }
    result
}
'''
new = '''fn stroke_edges(points: &[Vec2], closed: bool, half_width: f32) -> Vec<(Vec2, Vec2)> {
    let mut result = Vec::with_capacity(points.len());
    for index in 0..points.len() {
        let point = points[index];
        let previous = if index > 0 {
            points[index - 1]
        } else if closed {
            points[points.len() - 1]
        } else {
            point
        };
        let next = if index + 1 < points.len() {
            points[index + 1]
        } else if closed {
            points[0]
        } else {
            point
        };

        let offset = if !closed && index == 0 {
            segment_normal(point, next, half_width)
        } else if !closed && index + 1 == points.len() {
            segment_normal(previous, point, half_width)
        } else {
            miter_offset(previous, point, next, half_width)
        };
        result.push((
            Vec2::new(point.x + offset.x, point.y + offset.y),
            Vec2::new(point.x - offset.x, point.y - offset.y),
        ));
    }
    result
}

fn segment_normal(from: Vec2, to: Vec2, half_width: f32) -> Vec2 {
    let tangent = normalized(Vec2::new(to.x - from.x, to.y - from.y));
    Vec2::new(-tangent.y * half_width, tangent.x * half_width)
}

fn miter_offset(previous: Vec2, point: Vec2, next: Vec2, half_width: f32) -> Vec2 {
    let incoming = normalized(Vec2::new(point.x - previous.x, point.y - previous.y));
    let outgoing = normalized(Vec2::new(next.x - point.x, next.y - point.y));
    let incoming_normal = Vec2::new(-incoming.y, incoming.x);
    let outgoing_normal = Vec2::new(-outgoing.y, outgoing.x);
    let summed = Vec2::new(
        incoming_normal.x + outgoing_normal.x,
        incoming_normal.y + outgoing_normal.y,
    );
    let summed_length = summed.x.hypot(summed.y);
    if summed_length <= f32::EPSILON {
        return Vec2::new(
            outgoing_normal.x * half_width,
            outgoing_normal.y * half_width,
        );
    }

    let miter = Vec2::new(summed.x / summed_length, summed.y / summed_length);
    let alignment = (miter.x * outgoing_normal.x + miter.y * outgoing_normal.y).abs();
    if alignment <= f32::EPSILON {
        return Vec2::new(
            outgoing_normal.x * half_width,
            outgoing_normal.y * half_width,
        );
    }

    let length = (half_width / alignment).min(half_width * MORPH_MITER_LIMIT);
    Vec2::new(miter.x * length, miter.y * length)
}
'''
if old not in text:
    raise SystemExit("stroke_edges implementation marker missing")
text = text.replace(old, new, 1)

marker = '''    #[test]
    fn morph_tessellation_has_fixed_dual_position_topology() {
'''
test = '''    #[test]
    fn morph_closed_stroke_seam_preserves_constant_width_at_sharp_join() {
        let half_width = 0.1;
        let points = vec![
            Vec2::new(0.0, 2.0),
            Vec2::new(0.47, 0.65),
            Vec2::new(1.9, 0.62),
            Vec2::new(0.76, -0.25),
            Vec2::new(1.18, -1.62),
            Vec2::new(0.0, -0.82),
            Vec2::new(-1.18, -1.62),
            Vec2::new(-0.76, -0.25),
            Vec2::new(-1.9, 0.62),
            Vec2::new(-0.47, 0.65),
        ];
        let edges = stroke_edges(&points, true, half_width);
        let point = points[0];
        let previous = points[points.len() - 1];
        let next = points[1];
        let incoming = normalized(Vec2::new(point.x - previous.x, point.y - previous.y));
        let outgoing = normalized(Vec2::new(next.x - point.x, next.y - point.y));
        let incoming_normal = Vec2::new(-incoming.y, incoming.x);
        let outgoing_normal = Vec2::new(-outgoing.y, outgoing.x);
        let (left, right) = edges[0];
        let outer = if left.y > right.y { left } else { right };
        let offset = Vec2::new(outer.x - point.x, outer.y - point.y);

        let incoming_distance =
            (offset.x * incoming_normal.x + offset.y * incoming_normal.y).abs();
        let outgoing_distance =
            (offset.x * outgoing_normal.x + offset.y * outgoing_normal.y).abs();
        assert!((incoming_distance - half_width).abs() < 1e-5);
        assert!((outgoing_distance - half_width).abs() < 1e-5);
        assert!(outer.y > point.y + half_width * 2.0);
    }

    #[test]
    fn morph_tessellation_has_fixed_dual_position_topology() {
'''
if marker not in text:
    raise SystemExit("morph tessellation test marker missing")
text = text.replace(marker, test, 1)

path.write_text(text)
