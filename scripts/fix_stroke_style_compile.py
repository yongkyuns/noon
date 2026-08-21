from pathlib import Path
import re

# Rust struct-update syntax: the base expression is always final and has no comma.
for rust_file in Path("crates").rglob("*.rs"):
    text = rust_file.read_text()
    text = re.sub(r"(?m)^(\s*\.\.[^,\n]+),\s*$", r"\1", text)
    rust_file.write_text(text)

# Preserve the older closed-seam theoretical oracle as a test-only helper. The
# production morph mesh no longer uses this strip construction, but the oracle
# still independently verifies the miter intersection math used by the new join.
geometry = Path("crates/noon-geometry/src/tessellation.rs")
text = geometry.read_text()
marker = "fn normalized(vector: Vec2) -> Vec2 {"
helpers = r'''#[cfg(test)]
fn stroke_edges(points: &[Vec2], closed: bool, half_width: f32) -> Vec<(Vec2, Vec2)> {
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
            test_segment_normal(point, next, half_width)
        } else if !closed && index + 1 == points.len() {
            test_segment_normal(previous, point, half_width)
        } else {
            test_miter_offset(previous, point, next, half_width)
        };
        result.push((
            Vec2::new(point.x + offset.x, point.y + offset.y),
            Vec2::new(point.x - offset.x, point.y - offset.y),
        ));
    }
    result
}

#[cfg(test)]
fn test_segment_normal(from: Vec2, to: Vec2, half_width: f32) -> Vec2 {
    let tangent = normalized(Vec2::new(to.x - from.x, to.y - from.y));
    Vec2::new(-tangent.y * half_width, tangent.x * half_width)
}

#[cfg(test)]
fn test_miter_offset(previous: Vec2, point: Vec2, next: Vec2, half_width: f32) -> Vec2 {
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
if "fn test_miter_offset(" not in text:
    if marker not in text:
        raise SystemExit("normalized marker missing from tessellation.rs")
    text = text.replace(marker, helpers + marker, 1)
geometry.write_text(text)

# Exercise the actual compile/runtime/preparer pipeline for cache identity.
Path("crates/noon-render-wgpu/tests/stroke_style_cache.rs").write_text(r'''use noon_compile::CompiledScene;
use noon_core::{Color, GeometryRef, SceneDefinition, StrokeCap, StrokeJoin, Style, Vec2, VectorPath};
use noon_render_wgpu::FramePreparer;
use noon_runtime::SceneInstance;

fn style(join: StrokeJoin, cap: StrokeCap) -> Style {
    Style {
        fill: None,
        stroke: Some(Color::WHITE),
        stroke_width: 0.2,
        stroke_join: join,
        stroke_cap: cap,
        opacity: 1.0,
    }
}

#[test]
fn path_cache_key_includes_join_and_cap_policy() {
    let path = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::ZERO)
        .line_to(Vec2::new(1.0, 1.0));
    let styles = [
        style(StrokeJoin::Round, StrokeCap::Round),
        style(StrokeJoin::Miter, StrokeCap::Round),
        style(StrokeJoin::Round, StrokeCap::Butt),
        style(StrokeJoin::Round, StrokeCap::Round),
    ];
    let mut scene = SceneDefinition::new();
    for path_style in styles {
        let object = scene.add(GeometryRef::path(path.clone()));
        scene.object_mut(object).unwrap().style = path_style;
    }
    let instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    let mut preparer = FramePreparer::new();
    let prepared = preparer.prepare(instance.frame());
    assert_eq!(prepared.stats.geometry_cache_misses, 3);
    assert_eq!(preparer.cached_path_mesh_count(), 3);
}
''')

# Pair-based topology tests were replaced by geometry invariants, so the old
# midpoint helper is intentionally removed to keep the strict Clippy gate clean.
correctness = Path("crates/noon-geometry/tests/tessellation_correctness.rs")
text = correctness.read_text()
text = text.replace(
    '''fn midpoint(a: Vec2, b: Vec2) -> Vec2 {\n    scale(add(a, b), 0.5)\n}\n\n''',
    "",
    1,
)
correctness.write_text(text)
