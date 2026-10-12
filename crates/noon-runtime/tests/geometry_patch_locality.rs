use noon_compile::{CompiledObject, CompiledScene, ExecutionPatch};
use noon_core::{GeometryRef, ObjectContentRef, ObjectId, Style, Transform2D, Vec2};
use noon_runtime::{
    RuntimePatchStats, SceneInstance, TransientPresentationOccurrence, TransientPresentationState,
};

const OBJECT_COUNT: usize = 100_000;
const TARGET_INDEX: usize = OBJECT_COUNT / 2;
const UNTOUCHED_INDEX: usize = TARGET_INDEX + 1;

#[test]
fn geometry_patch_touches_only_one_object_in_a_100k_scene() {
    let objects: Vec<_> = (0..OBJECT_COUNT)
        .map(|index| ObjectId::new(index as u64))
        .collect();
    let compiled_objects = objects
        .iter()
        .map(|&id| {
            CompiledObject::new(
                id,
                GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                Style::default(),
            )
        })
        .collect();
    let compiled = CompiledScene::compile_objects(compiled_objects, &[])
        .expect("static execution data compiles");
    let mut live = SceneInstance::new(compiled);
    live.take_frame_changes();

    let target = objects[TARGET_INDEX];
    let untouched_before = live.frame().objects[UNTOUCHED_INDEX].clone();
    let replacement = GeometryRef::line(Vec2::new(-2.0, 1.0), Vec2::new(3.0, -1.0));

    live.apply_execution_patch(&ExecutionPatch::SetContent {
        object: target,
        content: replacement.clone().into(),
        text_bounds: None,
    })
    .expect("local geometry patch must succeed");

    assert_eq!(live.last_patch_stats(), RuntimePatchStats::default());
    assert_eq!(live.take_frame_changes().object_indices(), &[TARGET_INDEX]);
    assert_eq!(
        live.frame().objects[TARGET_INDEX].geometry(),
        Some(&replacement)
    );
    assert_eq!(live.frame().objects[UNTOUCHED_INDEX], untouched_before);
    assert_eq!(live.frame().objects.len(), OBJECT_COUNT);
}

#[test]
fn transient_presentation_retirement_does_not_dirty_a_100k_stable_scene() {
    let compiled_objects = (0..OBJECT_COUNT)
        .map(|index| {
            CompiledObject::new(
                ObjectId::new(index as u64),
                GeometryRef::circle(1.0),
                Transform2D::IDENTITY,
                Style::default(),
            )
        })
        .collect();
    let compiled = CompiledScene::compile_objects(compiled_objects, &[])
        .expect("static execution data compiles");
    let mut live = SceneInstance::new(compiled);

    {
        let initial = live.take_renderer_publication();
        assert!(initial.changes().is_all());
    }
    assert!(live.take_spatial_changes().is_all());

    let transient = [TransientPresentationOccurrence::new(
        0,
        7,
        TransientPresentationState {
            z_index: 0.0,
            content: ObjectContentRef::Geometry(GeometryRef::circle(0.5)),
            text_bounds: None,
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
            presence: true,
            reveal: 1.0,
            morph: 0.0,
            render_geometry: None,
            render_transform: None,
        },
    )];
    {
        let endpoint = live
            .take_renderer_publication_with_followup_presentation_redraw()
            .with_transient_presentations(&transient)
            .expect("valid transient presentation");
        assert!(endpoint.changes().is_empty());
        assert_eq!(endpoint.transient_presentations().len(), 1);
    }

    let retirement = live.take_renderer_publication();
    let changes = retirement.changes();
    assert!(!changes.is_all());
    assert!(changes.requires_presentation_redraw());
    assert!(!changes.has_stable_changes());
    assert!(!changes.is_structural());
    assert!(!changes.has_painter_order_change());
    assert!(changes.object_indices().is_empty());
    assert!(changes.added_indices().is_empty());
    assert!(changes.removed_indices().is_empty());
    assert_eq!(retirement.frame().objects.len(), OBJECT_COUNT);
    assert!(retirement.transient_presentations().is_empty());
    assert!(live.take_spatial_changes().is_empty());
}
