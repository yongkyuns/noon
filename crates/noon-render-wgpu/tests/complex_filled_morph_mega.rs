use noon_core::{GeometryRef, ObjectContentRef, ObjectId, Style, Transform2D, Vec2, VectorPath};
use noon_render_wgpu::FramePreparer;
use noon_runtime::{FrameChanges, FrameObjectState, FrameState};

fn source() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .line_to(Vec2::new(8.0, 0.0))
        .line_to(Vec2::new(8.0, 8.0))
        .line_to(Vec2::new(6.0, 8.0))
        .line_to(Vec2::new(6.0, 2.0))
        .line_to(Vec2::new(2.0, 2.0))
        .line_to(Vec2::new(2.0, 8.0))
        .line_to(Vec2::new(0.0, 8.0))
        .close()
}

fn static_target() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .line_to(Vec2::new(8.0, 0.0))
        .line_to(Vec2::new(8.0, 8.0))
        .line_to(Vec2::new(0.0, 8.0))
        .close()
}

fn rounded_target() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .cubic_to(
            Vec2::new(2.0, -10.0),
            Vec2::new(6.0, -10.0),
            Vec2::new(8.0, 0.0),
        )
        .cubic_to(
            Vec2::new(18.0, 2.0),
            Vec2::new(18.0, 6.0),
            Vec2::new(8.0, 8.0),
        )
        .cubic_to(
            Vec2::new(6.0, 18.0),
            Vec2::new(2.0, 18.0),
            Vec2::new(0.0, 8.0),
        )
        .cubic_to(
            Vec2::new(-10.0, 6.0),
            Vec2::new(-10.0, 2.0),
            Vec2::new(0.0, 0.0),
        )
        .close()
}

fn mixed_frame(progress: f32, curved: bool) -> FrameState {
    let morph_target = if curved {
        rounded_target()
    } else {
        static_target()
    };
    let mut static_transform = Transform2D::IDENTITY;
    static_transform.translation = Vec2::new(20.0, 0.0);
    FrameState {
        time: f64::from(progress),
        objects: vec![
            FrameObjectState {
                id: ObjectId::new(0),
                z_index: 0.0,
                content: ObjectContentRef::Geometry(GeometryRef::path(
                    source().with_morph_target(morph_target),
                )),
                text_bounds: None,
                transform: Transform2D::IDENTITY,
                style: Style {
                    stroke: None,
                    stroke_width: 0.0,
                    ..Style::default()
                },
                appearance: 1.0,
            },
            FrameObjectState {
                id: ObjectId::new(1),
                z_index: 0.0,
                content: ObjectContentRef::Geometry(GeometryRef::path(static_target())),
                text_bounds: None,
                transform: static_transform,
                style: Style {
                    stroke: None,
                    stroke_width: 0.0,
                    ..Style::default()
                },
                appearance: 1.0,
            },
        ],
        presences: vec![true, true],
        reveals: vec![1.0, 1.0],
        morphs: vec![progress, 0.0],
        render_geometries: vec![None, None],
        render_transforms: vec![None, None],
        family_animations: Vec::new(),
        family_animation_plan_indices: Vec::new(),
    }
}

#[test]
fn sampled_fill_paint_does_not_dirty_unowned_mega_attributes() {
    let mut preparer = FramePreparer::new();
    let initial = mixed_frame(0.5, false);
    let prepared = preparer.prepare(&initial);
    assert_eq!(prepared.stats.unsupported_count, 0);
    assert_eq!(prepared.stats.mega_path_count, 1);

    let mut changed = initial;
    changed.objects[0].style.opacity = 0.4;
    let prepared = preparer.prepare_incremental(&changed, &FrameChanges::objects(vec![0]));
    assert!(prepared.mega_path_instance_dirty_ranges.is_empty());
}

#[test]
fn growing_sampled_fill_does_not_index_the_fixed_mega_attribute_buffer() {
    let mut preparer = FramePreparer::new();
    let initial = mixed_frame(0.0, true);
    let prepared = preparer.prepare(&initial);
    assert_eq!(prepared.stats.unsupported_count, 0);
    assert_eq!(prepared.stats.mega_path_count, 1);

    for progress in [0.1, 0.25, 0.5, 0.75, 1.0] {
        let current = mixed_frame(progress, true);
        let prepared = preparer.prepare_incremental(&current, &FrameChanges::objects(vec![0]));
        assert_eq!(prepared.stats.unsupported_count, 0);
        assert!(prepared.mega_path_instance_dirty_ranges.is_empty());
    }
}
