from pathlib import Path


def replace(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    if old not in text:
        raise SystemExit(f"missing patch anchor in {path}: {old[:160]!r}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


replace(
    "crates/noon-render-wgpu/src/lib.rs",
    '''        let stroke_width_mode = match value.stroke_width_mode {
            StrokeWidthMode::ScaleWithObject => 0,
            StrokeWidthMode::ScreenSpace => 2,
        };
        Self {
            fill,
            stroke,
            stroke_width: value.stroke_width,
            opacity: value.opacity,
            fill_enabled,
            stroke_enabled: stroke_enabled | stroke_width_mode,
        }
''',
    '''        let stroke_width_mode = match value.stroke_width_mode {
            StrokeWidthMode::ScaleWithObject => 0,
            StrokeWidthMode::ScreenSpace => 2,
        };
        // Low bits retain the existing enabled/screen-space contract. Analytic
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
)

replace(
    "crates/noon-render-wgpu/src/analytic.wgsl",
    '''fn stroke_is_screen_space(flags: vec2<u32>) -> bool {
    return (flags.y & 2u) != 0u;
}
''',
    '''fn stroke_is_screen_space(flags: vec2<u32>) -> bool {
    return (flags.y & 2u) != 0u;
}

fn stroke_cap_mode(flags: vec2<f32>) -> u32 {
    return (u32(flags.y) >> 2u) & 3u;
}
''',
)

replace(
    "crates/noon-render-wgpu/src/analytic.wgsl",
    '''@fragment
fn fs_line(input: VertexOutput) -> @location(0) vec4<f32> {
    let half_length = input.geometry.x * 0.5;
    let radius = input.geometry.y * 0.5;
    let signed_distance = capsule_signed_distance(input.local, half_length, radius);
    let visible = select(0.0, 1.0, input.geometry.y > 0.0);
    return styled_line_color(input, signed_distance) * visible;
}
''',
    '''@fragment
fn fs_line(input: VertexOutput) -> @location(0) vec4<f32> {
    let half_length = input.geometry.x * 0.5;
    let radius = input.geometry.y * 0.5;
    let cap_mode = stroke_cap_mode(input.flags);
    var signed_distance = capsule_signed_distance(input.local, half_length, radius);
    if cap_mode == 1u {
        // Cairo/Manim BUTT caps terminate at the semantic endpoints. The proxy
        // quad is intentionally larger for antialiasing; the SDF clips coverage.
        signed_distance = rectangle_signed_distance(
            input.local,
            vec2<f32>(half_length, radius),
        );
    } else if cap_mode == 2u {
        // SQUARE extends by exactly one half stroke width along the tangent.
        signed_distance = rectangle_signed_distance(
            input.local,
            vec2<f32>(half_length + radius, radius),
        );
    }
    let visible = select(0.0, 1.0, input.geometry.y > 0.0);
    return styled_line_color(input, signed_distance) * visible;
}
''',
)

Path("crates/noon-render-wgpu/tests/analytic_line_caps.rs").write_text(
    '''use noon_core::{\n    Color, GeometryRef, ObjectId, StrokeCap, StrokeWidthMode, Style, Transform2D, Vec2,\n};\nuse noon_render_wgpu::FramePreparer;\nuse noon_runtime::{FrameObjectState, FrameState};\n\nfn line_frame(cap: StrokeCap, width_mode: StrokeWidthMode) -> FrameState {\n    FrameState {\n        time: 0.0,\n        objects: vec![FrameObjectState {\n            id: ObjectId::new(0),\n            geometry: GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::ZERO),\n            transform: Transform2D::IDENTITY,\n            style: Style {\n                fill: None,\n                stroke: Some(Color::WHITE),\n                stroke_width: 0.04,\n                stroke_width_mode: width_mode,\n                stroke_join: Default::default(),\n                stroke_cap: cap,\n                opacity: 1.0,\n            },\n            appearance: 1.0,\n        }],\n        presences: vec![true],\n        reveals: vec![1.0],\n        morphs: vec![0.0],\n        render_geometries: vec![None],\n    }\n}\n\n#[test]\nfn analytic_line_packs_all_cap_modes_without_changing_existing_flag_bits() {\n    for (cap, expected_cap_bits) in [\n        (StrokeCap::Round, 0u32),\n        (StrokeCap::Butt, 1u32 << 2),\n        (StrokeCap::Square, 2u32 << 2),\n    ] {\n        for (width_mode, expected_width_bit) in [\n            (StrokeWidthMode::ScaleWithObject, 0u32),\n            (StrokeWidthMode::ScreenSpace, 2u32),\n        ] {\n            let frame = line_frame(cap, width_mode);\n            let mut preparer = FramePreparer::new();\n            let prepared = preparer.prepare(&frame);\n            let flags = prepared.lines[0].style.stroke_enabled;\n            assert_eq!(flags & 1, 1, \"stroke enabled bit changed\");\n            assert_eq!(flags & 2, expected_width_bit, \"width-mode bit changed\");\n            assert_eq!(flags & 0b1100, expected_cap_bits, \"cap bits mismatch\");\n        }\n    }\n}\n\n#[test]\nfn analytic_shader_has_distinct_round_butt_and_square_cap_sdfs() {\n    let shader = include_str!(\"../src/analytic.wgsl\");\n    assert!(shader.contains(\"capsule_signed_distance(input.local, half_length, radius)\"));\n    assert!(shader.contains(\"if cap_mode == 1u\"));\n    assert!(shader.contains(\"vec2<f32>(half_length, radius)\"));\n    assert!(shader.contains(\"else if cap_mode == 2u\"));\n    assert!(shader.contains(\"vec2<f32>(half_length + radius, radius)\"));\n}\n''',
    encoding="utf-8",
)
