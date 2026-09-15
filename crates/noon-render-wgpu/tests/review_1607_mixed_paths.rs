// Review-only probe of PR #1607. Reuse its fixture so the same sampled
// fallback is tested alongside a genuinely mega-batched static path.
include!("complex_filled_morph.rs");

fn mixed_frame(progress: f32, curved: bool) -> FrameState {
    let mut result = frame(progress, 1);
    result.objects[1].content = ObjectContentRef::Geometry(GeometryRef::path(target()));
    result.objects[1].transform.translation = Vec2::new(20.0, 0.0);
    if curved {
        let rounded = VectorPath::new()
            .move_to(Vec2::new(0.0, 0.0))
            .cubic_to(Vec2::new(2.0, -10.0), Vec2::new(6.0, -10.0), Vec2::new(8.0, 0.0))
            .cubic_to(Vec2::new(18.0, 2.0), Vec2::new(18.0, 6.0), Vec2::new(8.0, 8.0))
            .cubic_to(Vec2::new(6.0, 18.0), Vec2::new(2.0, 18.0), Vec2::new(0.0, 8.0))
            .cubic_to(Vec2::new(-10.0, 6.0), Vec2::new(-10.0, 2.0), Vec2::new(0.0, 0.0))
            .close();
        result.objects[0].content = ObjectContentRef::Geometry(
            GeometryRef::path(source().with_morph_target(rounded)),
        );
    }
    result
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
    assert!(prepared.mega_path_instance_dirty_ranges.is_empty(),
        "sampled fill has no mega segment but dirtied {:?}", prepared.mega_path_instance_dirty_ranges);
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
