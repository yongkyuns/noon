from pathlib import Path


def replace(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    if old not in text:
        raise SystemExit(f"missing patch anchor in {path}: {old[:160]!r}")
    p.write_text(text.replace(old, new, 1), encoding="utf-8")


replace(
    "crates/noon-render-wgpu/src/lib.rs",
    '''        // Low bits retain the existing enabled/screen-space contract. Analytic
        // lines use bits 2-3 for the semantic cap mode without growing the packed
        // instance layout shared by native, WebGPU, and WebGL2 backends.
        let stroke_cap_mode = match value.stroke_cap {
            StrokeCap::Round => 0,
            StrokeCap::Butt => 1 << 2,
            StrokeCap::Square => 2 << 2,
        };
        Self {
            fill,
            stroke,
            stroke_width: value.stroke_width,
            opacity: value.opacity,
            fill_enabled,
            stroke_enabled: stroke_enabled | stroke_width_mode | stroke_cap_mode,
        }
''',
    '''        Self {
            fill,
            stroke,
            stroke_width: value.stroke_width,
            opacity: value.opacity,
            fill_enabled,
            stroke_enabled: stroke_enabled | stroke_width_mode,
        }
''',
)

replace(
    "crates/noon-render-wgpu/src/lib.rs",
    '''    let mut transform: PackedTransform = object.transform.into();
    transform.padding = reveal.clamp(0.0, 1.0);
    LineInstance {
        transform,
        style: pack_style(object),
        start: [start.x, start.y],
        end: [end.x, end.y],
    }
''',
    '''    let mut transform: PackedTransform = object.transform.into();
    transform.padding = reveal.clamp(0.0, 1.0);
    let mut style = pack_style(object);
    // Cap mode is line geometry, not a global style-packing concern. Keeping
    // these bits off circles/rectangles/paths preserves their exact packed bytes
    // and prevents line-cap semantics from perturbing unrelated raster paths.
    style.stroke_enabled |= match object.style.stroke_cap {
        StrokeCap::Round => 0,
        StrokeCap::Butt => 1 << 2,
        StrokeCap::Square => 2 << 2,
    };
    LineInstance {
        transform,
        style,
        start: [start.x, start.y],
        end: [end.x, end.y],
    }
''',
)

# Strengthen the focused test: cap bits must be line-local and rectangle packing
# must remain byte-compatible regardless of semantic cap metadata.
p = Path("crates/noon-render-wgpu/tests/analytic_line_caps.rs")
text = p.read_text(encoding="utf-8")
text = text.replace(
    '''#[test]
fn analytic_shader_has_distinct_round_butt_and_square_cap_sdfs() {
''',
    '''#[test]
fn rectangle_packing_does_not_carry_line_cap_bits() {
    for cap in [StrokeCap::Round, StrokeCap::Butt, StrokeCap::Square] {
        let mut frame = line_frame(cap, StrokeWidthMode::ScreenSpace);
        frame.objects[0].geometry = GeometryRef::rectangle(Vec2::new(2.0, 2.0));
        let mut preparer = FramePreparer::new();
        let prepared = preparer.prepare(&frame);
        let flags = prepared.rectangles[0].style.stroke_enabled;
        assert_eq!(flags, 3, "non-line packed flags must remain legacy-compatible");
    }
}

#[test]
fn analytic_shader_has_distinct_round_butt_and_square_cap_sdfs() {
''',
    1,
)
p.write_text(text, encoding="utf-8")
