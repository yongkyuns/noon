//! Independent broad-phase/rank references, with no renderer or host transport.
use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{GeometryRef, ObjectId, Rect, Style, Transform2D, Vec2};
use noon_runtime::{
    ExecutionSlotId, ExecutionSpatialIndex, PainterPrefixGeometryQueryError as Error, SceneInstance,
};
fn row(id: u64, x: f32, z: f64) -> CompiledObject {
    let mut value = CompiledObject::new(
        ObjectId::new(id),
        GeometryRef::circle(0.25),
        Transform2D {
            translation: Vec2::new(x, 0.0),
            ..Transform2D::IDENTITY
        },
        Style::default(),
    );
    value.base_z_index = z;
    value
}
#[test]
fn prefix_geometry_queries_match_independent_reference_without_visiting_twenty_thousand_rows() {
    let count = 20_000;
    let priority = |i: usize| ((i * 7919) % 65536) as f64;
    let mut runtime = SceneInstance::new(
        CompiledScene::compile_objects(
            (0..count)
                .map(|i| row(i as u64, i as f32 * 3.0, priority(i)))
                .collect(),
            &[],
        )
        .unwrap(),
    );
    let mut index = ExecutionSpatialIndex::default();
    index.rebuild(
        runtime.frame(),
        runtime.geometry_resources(),
        runtime
            .painter_order()
            .iter()
            .map(|&i| (ExecutionSlotId::new(i, 0), i as usize)),
    );
    let anchor = 19_000;
    let publication = runtime
        .take_renderer_publication()
        .with_spatial_index(&index);
    for sample in 0..96 {
        let center = ((sample * 197) % 19_990) as f32 * 3.0;
        let bounds = Rect::new(Vec2::new(center - 1.1, -1.0), Vec2::new(center + 1.1, 1.0));
        let mut expected = (0..count)
            .filter(|&i| {
                let x = i as f32 * 3.0;
                x + 0.25 >= bounds.min.x
                    && x - 0.25 <= bounds.max.x
                    && priority(i) < priority(anchor)
            })
            .collect::<Vec<_>>();
        expected.sort_unstable_by(|&a, &b| priority(a).total_cmp(&priority(b)));
        let expected = expected
            .into_iter()
            .map(|i| ObjectId::new(i as u64))
            .collect::<Vec<_>>();
        let actual = publication
            .query_painter_prefix_geometry(bounds, ObjectId::new(anchor as u64))
            .unwrap();
        assert_eq!(actual.objects, expected, "sample {sample}");
        assert_eq!(actual.stats.full_scan_fallbacks, 0);
        assert!(
            actual.stats.candidates_tested < 8,
            "sample {sample}: {:?}",
            actual.stats
        );
    }
}
#[test]
fn canonical_inverse_ranks_override_stale_spatial_sort_keys_and_reject_foreign_generations() {
    let mut runtime = SceneInstance::new(
        CompiledScene::compile_objects(
            vec![row(0, 0.0, 0.0), row(1, 0.1, 1.0), row(2, 0.2, 2.0)],
            &[],
        )
        .unwrap(),
    );
    let bounds = Rect::new(Vec2::new(-1.0, -1.0), Vec2::new(1.0, 1.0));
    let mut index = ExecutionSpatialIndex::default();
    for i in 0..3 {
        index.upsert_bounds(
            ExecutionSlotId::new(i, 0),
            ObjectId::new(i as u64),
            bounds,
            10 - i as u64,
        );
    }
    {
        let publication = runtime
            .take_renderer_publication()
            .with_spatial_index(&index);
        let actual = publication
            .query_painter_prefix_geometry(bounds, ObjectId::new(2))
            .unwrap();
        assert_eq!(actual.objects, [ObjectId::new(0), ObjectId::new(1)]);
        assert_eq!(
            index.query_rect(bounds).slots()[0],
            ExecutionSlotId::new(2, 0),
            "negative control really has reversed spatial sort keys"
        );
    }
    let foreign = ObjectId::new(1 << 32);
    index.upsert_bounds(ExecutionSlotId::new(0, 1), foreign, bounds, 0);
    let publication = runtime
        .take_renderer_publication()
        .with_spatial_index(&index);
    assert_eq!(
        publication.query_painter_prefix_geometry(bounds, ObjectId::new(2)),
        Err(Error::StaleSpatialEntry(foreign))
    );
}
