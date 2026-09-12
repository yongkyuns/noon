use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    GeometryRef, ObjectContentRef, ObjectId, Style, Transform2D, Vec2, VectorPath,
};
use noon_render_wgpu::{
    DerivedDisplayPrimitive, DisplayPainterItem, FramePreparer, TransientPresentationPreparer,
};
use noon_runtime::{
    FrameChanges, SceneInstance, TransientPresentationOccurrence, TransientPresentationState,
};

fn triangle(height: f32) -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(-0.6, -0.5))
        .line_to(Vec2::new(0.6, -0.5))
        .line_to(Vec2::new(0.0, height))
        .close()
}

fn transient_state(path: VectorPath) -> TransientPresentationState {
    TransientPresentationState {
        z_index: 0.0,
        content: ObjectContentRef::Geometry(GeometryRef::path(path)),
        text_bounds: None,
        transform: Transform2D::IDENTITY,
        style: Style::default(),
        appearance: 1.0,
        presence: true,
        reveal: 1.0,
        morph: 0.0,
        render_geometry: None,
        render_transform: None,
    }
}

fn runtime_with_stable_paths() -> SceneInstance {
    let objects = vec![
        CompiledObject::new(
            ObjectId::new(1),
            GeometryRef::path(triangle(0.8)),
            Transform2D::IDENTITY,
            Style::default(),
        ),
        CompiledObject::new(
            ObjectId::new(2),
            GeometryRef::path(triangle(1.2)),
            Transform2D {
                translation: Vec2::new(2.0, 0.0),
                ..Transform2D::IDENTITY
            },
            Style::default(),
        ),
    ];
    SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap())
}

#[test]
fn transient_path_reuses_preparation_without_touching_stable_path_residency() {
    let mut runtime = runtime_with_stable_paths();
    let mut stable_preparer = FramePreparer::new();
    let initial = stable_preparer.prepare(runtime.frame());
    let stable_vertices = initial.path_vertices.to_vec();
    let stable_indices = initial.path_indices.to_vec();
    drop(initial);
    let stable_cache_count = stable_preparer.cached_path_mesh_count();
    assert_eq!(stable_cache_count, 2);

    let target = triangle(1.4);
    let morph = triangle(0.7).with_morph_target(target);
    let mut state = transient_state(morph);
    state.morph = 0.4;
    let presentations = [TransientPresentationOccurrence::new(0, 9, state)];
    let publication = runtime
        .take_renderer_publication()
        .with_transient_presentations(&presentations)
        .unwrap();

    let mut transient_preparer = TransientPresentationPreparer::new();
    let first = transient_preparer.prepare(&publication).unwrap();
    assert_eq!(first.paths.len(), 1);
    assert!(!first.path_vertices.is_empty());
    assert!(!first.path_indices.is_empty());
    assert_eq!(transient_preparer.cached_path_mesh_count(), 1);
    assert_eq!(transient_preparer.path_cache_misses(), 1);
    assert_eq!(transient_preparer.path_cache_hits(), 0);
    let transient_vertices = first.path_vertices.clone();
    let transient_indices = first.path_indices.clone();

    let second = transient_preparer.prepare(&publication).unwrap();
    assert_eq!(second.path_vertices, transient_vertices);
    assert_eq!(second.path_indices, transient_indices);
    assert_eq!(transient_preparer.cached_path_mesh_count(), 1);
    assert_eq!(transient_preparer.path_cache_misses(), 1);
    assert_eq!(transient_preparer.path_cache_hits(), 1);

    let stable_after_transient =
        stable_preparer.prepare_incremental(publication.frame(), &FrameChanges::default());
    assert_eq!(stable_after_transient.stats.full_rebuilds, 0);
    assert_eq!(stable_after_transient.stats.geometry_cache_misses, 0);
    assert_eq!(stable_after_transient.stats.path_vertices_repacked, 0);
    assert_eq!(stable_after_transient.stats.path_indices_repacked, 0);
    assert_eq!(stable_after_transient.path_vertices, stable_vertices.as_slice());
    assert_eq!(stable_after_transient.path_indices, stable_indices.as_slice());
    drop(stable_after_transient);
    assert_eq!(stable_preparer.cached_path_mesh_count(), stable_cache_count);

    let retirement = runtime.take_renderer_publication();
    let retired = transient_preparer.prepare(&retirement).unwrap();
    assert!(retired.slots.is_empty());
    assert!(retired.paths.is_empty());
    assert!(retired.path_vertices.is_empty());
    assert!(retired.path_indices.is_empty());

    let stable_after_retirement =
        stable_preparer.prepare_incremental(retirement.frame(), &FrameChanges::default());
    assert_eq!(stable_after_retirement.stats.full_rebuilds, 0);
    assert_eq!(stable_after_retirement.stats.geometry_cache_misses, 0);
    assert_eq!(stable_after_retirement.stats.path_vertices_repacked, 0);
    assert_eq!(stable_after_retirement.stats.path_indices_repacked, 0);
    assert_eq!(stable_preparer.cached_path_mesh_count(), stable_cache_count);
}

#[test]
fn transient_path_viewport_projection_keeps_anchor_order_and_identity_local() {
    let mut runtime = runtime_with_stable_paths();
    let presentations = [TransientPresentationOccurrence::new(
        1,
        17,
        transient_state(triangle(0.9)),
    )];
    let publication = runtime
        .take_renderer_publication()
        .with_transient_presentations(&presentations)
        .unwrap();
    let mut preparer = TransientPresentationPreparer::new();

    let first_only = preparer.prepare_visible(&publication, &[0]).unwrap();
    assert!(first_only.slots.is_empty());
    assert!(first_only.paths.is_empty());
    assert_eq!(
        first_only.painter_items,
        vec![DisplayPainterItem::Stable { object_index: 0 }]
    );

    let second_only = preparer.prepare_visible(&publication, &[1]).unwrap();
    assert_eq!(second_only.paths.len(), 1);
    assert_eq!(second_only.slots.len(), 1);
    assert_eq!(second_only.slots[0].anchor_object_index, 1);
    assert_eq!(second_only.slots[0].occurrence_index, 17);
    assert!(matches!(
        second_only.slots[0].primitive,
        DerivedDisplayPrimitive::Path { batch: 0, .. }
    ));
    assert_eq!(
        second_only.painter_items,
        vec![
            DisplayPainterItem::Stable { object_index: 1 },
            DisplayPainterItem::Derived {
                occurrence_index: 17,
            },
        ]
    );
}
