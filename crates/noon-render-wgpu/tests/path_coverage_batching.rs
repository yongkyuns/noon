use noon_core::{
    Color, GeometryRef, ObjectContentRef, ObjectId, StrokeCap, StrokeJoin, Style, Transform2D,
    Vec2, VectorPath,
};
use noon_render_wgpu::{FramePreparer, PathCoverage, RenderPrimitive};
use noon_runtime::{FrameObjectState, FrameState};

#[test]
fn mixed_paths_preserve_order_and_only_coalesce_matching_coverage_profiles() {
    let geometries = (0..4).map(|index| {
        let x = index as f32 * 2.0;
        let path = VectorPath::new().move_to(Vec2::new(x, 0.0));
        if index == 1 {
            path.cubic_to(
                Vec2::new(x + 0.2, 0.5),
                Vec2::new(x + 0.8, -0.5),
                Vec2::new(x + 1.0, 0.0),
            )
        } else {
            path.line_to(Vec2::new(x + 1.0, 0.0))
        }
    });
    let objects = geometries
        .enumerate()
        .map(|(index, path)| FrameObjectState {
            id: ObjectId::new(index as u64),
            content: ObjectContentRef::Geometry(GeometryRef::path(path)),
            text_bounds: None,
            glow: None,
            spatial: None,
            z_index: 0.0,
            transform: Transform2D::IDENTITY,
            style: Style {
                fill: None,
                stroke: Some(Color::WHITE),
                stroke_width: 0.04,
                stroke_join: StrokeJoin::Miter,
                stroke_cap: StrokeCap::Butt,
                ..Style::default()
            },
            appearance: 1.0,
        })
        .collect();
    let mut frame = FrameState {
        time: 0.0,
        objects,
        presences: vec![true; 4],
        reveals: vec![1.0; 4],
        morphs: vec![0.0; 4],
        render_geometries: vec![None; 4],
        render_transforms: vec![None; 4],
        family_animations: Vec::new(),
        family_animation_plan_indices: Vec::new(),
    };
    let mut preparer = FramePreparer::new();
    let indices = {
        let prepared = preparer.prepare(&frame);
        let ordered = prepared.ordered_render_batches().collect::<Vec<_>>();
        let mega = ordered
            .iter()
            .map(|b| b.mega_path_batch.expect("packed path"))
            .collect::<Vec<_>>();
        assert_eq!(
            mega.iter()
                .map(|b| (b.coverage, b.path_count))
                .collect::<Vec<_>>(),
            [
                (PathCoverage::PolygonalStroke, 1),
                (PathCoverage::Tessellated, 1),
                (PathCoverage::PolygonalStroke, 2)
            ]
        );
        assert_eq!(
            ordered
                .iter()
                .map(|b| b.batch.primitive)
                .collect::<Vec<_>>(),
            [
                RenderPrimitive::MegaPath { batch: 0 },
                RenderPrimitive::MegaPath { batch: 1 },
                RenderPrimitive::MegaPath { batch: 2 }
            ]
        );
        assert!(mega
            .windows(2)
            .all(|pair| pair[0].index_range.end == pair[1].index_range.start));
        prepared.mega_path_indices.to_vec()
    };
    // Revealing a stroke uses its original triangles without rebuilding the
    // resident mesh or routing the neighboring curve through polygon coverage.
    frame.reveals[0] = 0.4;
    let changed = preparer.prepare(&frame);
    assert_eq!(changed.stats.geometry_cache_misses, 0);
    assert_eq!(changed.stats.path_vertices_repacked, 0);
    assert_eq!(changed.mega_path_indices, indices);
    assert_eq!(
        changed
            .ordered_render_batches()
            .nth(1)
            .unwrap()
            .mega_path_batch
            .unwrap()
            .coverage,
        PathCoverage::Tessellated
    );
}
