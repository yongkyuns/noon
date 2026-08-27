use noon_core::{
    Color, GeometryRef, ObjectId, StrokeCap, StrokeWidthMode, Style, Transform2D, Vec2,
};
use noon_render_wgpu::FramePreparer;
use noon_runtime::{FrameObjectState, FrameState};

fn line_frame(cap: StrokeCap, width_mode: StrokeWidthMode) -> FrameState {
    FrameState {
        time: 0.0,
        objects: vec![FrameObjectState {
            id: ObjectId::new(0),
            geometry: GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::ZERO),
            transform: Transform2D::IDENTITY,
            style: Style {
                fill: None,
                stroke: Some(Color::WHITE),
                stroke_width: 0.04,
                stroke_width_mode: width_mode,
                stroke_join: Default::default(),
                stroke_cap: cap,
                opacity: 1.0,
            },
            appearance: 1.0,
        }],
        presences: vec![true],
        reveals: vec![1.0],
        morphs: vec![0.0],
        render_geometries: vec![None],
    }
}

#[test]
fn analytic_line_packs_all_cap_modes_without_changing_existing_flag_bits() {
    for (cap, expected_cap_bits) in [
        (StrokeCap::Round, 0u32),
        (StrokeCap::Butt, 1u32 << 2),
        (StrokeCap::Square, 2u32 << 2),
    ] {
        for (width_mode, expected_width_bit) in [
            (StrokeWidthMode::ScaleWithObject, 0u32),
            (StrokeWidthMode::ScreenSpace, 2u32),
        ] {
            let frame = line_frame(cap, width_mode);
            let mut preparer = FramePreparer::new();
            let prepared = preparer.prepare(&frame);
            let flags = prepared.lines[0].style.stroke_enabled;
            assert_eq!(flags & 1, 1, "stroke enabled bit changed");
            assert_eq!(flags & 2, expected_width_bit, "width-mode bit changed");
            assert_eq!(flags & 0b1100, expected_cap_bits, "cap bits mismatch");
        }
    }
}

#[test]
fn analytic_shader_has_distinct_round_butt_and_square_cap_sdfs() {
    let shader = include_str!("../src/analytic.wgsl");
    assert!(shader.contains("capsule_signed_distance(input.local, half_length, radius)"));
    assert!(shader.contains("if cap_mode == 1u"));
    assert!(shader.contains("vec2<f32>(half_length, radius)"));
    assert!(shader.contains("else if cap_mode == 2u"));
    assert!(shader.contains("vec2<f32>(half_length + radius, radius)"));
}
