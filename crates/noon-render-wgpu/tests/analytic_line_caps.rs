use noon_core::{
    Color, GeometryRef, ObjectId, StrokeCap, StrokeWidthMode, Style, Transform2D, Vec2,
};
use noon_render_wgpu::FramePreparer;
use noon_runtime::{FrameObjectState, FrameState};

fn line_frame(cap: StrokeCap, width_mode: StrokeWidthMode) -> FrameState {
    FrameState {
        family_animations: Vec::new(),
        family_animation_plan_indices: Vec::new(),
        time: 0.0,
        objects: vec![FrameObjectState {
            draw_enabled: true,
            glow: None,
            spatial: None,
            z_index: 0.0,
            id: ObjectId::new(0),
            content: noon_core::ObjectContentRef::Geometry(GeometryRef::line(
                Vec2::new(-1.0, 0.0),
                Vec2::ZERO,
            )),
            text_bounds: None,
            transform: Transform2D::IDENTITY,
            style: Style {
                fill: None,
                stroke: Some(Color::WHITE),
                stroke_width: 0.04,
                stroke_width_mode: width_mode,
                stroke_join: Default::default(),
                stroke_cap: cap,
                opacity: 1.0,
                ..Style::default()
            },
            appearance: 1.0,
        }],
        presences: vec![true],
        reveals: vec![1.0],
        morphs: vec![0.0],
        render_geometries: vec![None],
        render_transforms: vec![None],
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
            let mut preparer = FramePreparer::new(&wgpu::Limits::default());
            let prepared = preparer.prepare(&frame);
            let flags = prepared.lines[0].style.stroke_enabled;
            assert_eq!(flags & 1, 1, "stroke enabled bit changed");
            assert_eq!(flags & 2, expected_width_bit, "width-mode bit changed");
            assert_eq!(flags & 0b1100, expected_cap_bits, "cap bits mismatch");
        }
    }
}

#[test]
fn rectangle_packing_does_not_carry_line_cap_bits() {
    for cap in [StrokeCap::Round, StrokeCap::Butt, StrokeCap::Square] {
        let mut frame = line_frame(cap, StrokeWidthMode::ScreenSpace);
        frame.objects[0].content =
            noon_core::ObjectContentRef::Geometry(GeometryRef::rectangle(2.0, 2.0));
        let mut preparer = FramePreparer::new(&wgpu::Limits::default());
        let prepared = preparer.prepare(&frame);
        let flags = prepared.rectangles[0].style.stroke_enabled;
        assert_eq!(
            flags, 3,
            "rectangle packed flags must remain legacy-compatible"
        );
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

#[test]
fn background_strokes_promote_analytic_primitives_and_invalidate_width_caches() {
    for geometry in [
        GeometryRef::circle(0.5),
        GeometryRef::rectangle(1.0, 1.0),
        GeometryRef::line(Vec2::new(-0.5, 0.0), Vec2::new(0.5, 0.0)),
    ] {
        let mut frame = line_frame(StrokeCap::Round, StrokeWidthMode::ScaleWithObject);
        frame.objects[0].content = noon_core::ObjectContentRef::Geometry(geometry);
        frame.objects[0].style.background_stroke = Some(Color::BLUE);
        frame.objects[0].style.background_stroke_width = 0.4;
        let mut preparer = FramePreparer::new();
        let initial = preparer.prepare(&frame);
        assert_eq!(initial.paths.len(), 1);
        assert!(initial.circles.is_empty());
        assert!(initial.rectangles.is_empty());
        assert!(initial.lines.is_empty());
        let max_y = |vertices: &[noon_render_wgpu::PathVertex]| {
            vertices
                .iter()
                .map(|v| v.position[1].abs())
                .fold(0.0, f32::max)
        };
        let before = max_y(initial.path_vertices);
        assert!(initial
            .path_vertices
            .iter()
            .any(|v| v.surface & (1 << 31) != 0));

        frame.objects[0].style.background_stroke_width = 0.8;
        let changed = noon_runtime::FrameChanges::objects(vec![0]);
        let updated = preparer.prepare_incremental(&frame, &changed);
        assert_eq!(updated.stats.geometry_cache_misses, 1);
        assert!(max_y(updated.path_vertices) > before + 0.15);

        frame.objects[0].style.background_stroke_width = 0.0;
        let removed = preparer.prepare_incremental(&frame, &changed);
        assert!(removed.paths.is_empty());
    }
}

#[test]
fn screen_space_background_stroke_width_is_independent_of_object_scale() {
    let mut frame = line_frame(StrokeCap::Round, StrokeWidthMode::ScreenSpace);
    frame.objects[0].style.background_stroke = Some(Color::BLUE);
    frame.objects[0].style.background_stroke_width = 0.4;
    frame.objects[0].transform.scale = Vec2::new(3.0, 2.0);
    let mut preparer = FramePreparer::new();
    let prepared = preparer.prepare(&frame);
    let half_width = prepared
        .path_vertices
        .iter()
        .filter(|v| v.surface & (1 << 31) != 0)
        .map(|v| v.position[1].abs())
        .fold(0.0, f32::max);
    assert!(
        (half_width - 0.2).abs() < 1e-6,
        "background half width: {half_width}"
    );
}

#[test]
fn analytic_line_fragment_passes_stroke_enablement_as_a_scalar_varying() {
    let shader = include_str!("../src/analytic.wgsl");
    assert!(shader.contains("@location(7) line_stroke_enabled: f32"));
    assert!(shader.contains(
        "output.line_stroke_enabled = select(0.0, 1.0, stroke_is_enabled(input.flags));"
    ));
    let styled_line_color = shader
        .split_once("fn styled_line_color")
        .and_then(|(_, remainder)| remainder.split_once("fn rectangle_signed_distance"))
        .map(|(body, _)| body)
        .expect("styled_line_color and the following helper must remain in the shader");
    assert!(styled_line_color.contains("input.line_stroke_enabled >= 0.5"));
    assert!(!styled_line_color.contains("(u32(input.flags.y) & 1u)"));
}
