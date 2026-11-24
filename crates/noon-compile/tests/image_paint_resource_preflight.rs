use std::sync::Arc;

use noon_compile::{
    CompilePatchError, CompiledObject, CompiledResourceError, CompiledResources, CompiledScene,
    ExecutionMutationTransaction, ExecutionPatch,
};
use noon_core::{
    Color, GeometryRef, ImageMapping, ImagePaint, ObjectId, PaintTransition, Property,
    RasterImageResourceArena, RasterImageResourceLookup, RasterImageSampling, RateFunction,
    SemanticPaint, Style, TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D,
};

#[test]
fn replacing_fill_track_requires_pixels_before_admitting_the_transaction() {
    let object = ObjectId::new(0);
    let original = TrackDefinition {
        id: TrackId::new(0),
        object,
        property: Property::Fill,
        values: TrackValues::Color {
            from: Some(Color::RED),
            to: Some(Color::BLUE),
        },
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        time_map: Default::default(),
    };
    let mut scene = CompiledScene::compile_objects(
        vec![CompiledObject::new(
            object,
            GeometryRef::circle(0.5),
            Transform2D::IDENTITY,
            Style::default(),
        )],
        std::slice::from_ref(&original),
    )
    .unwrap();
    let before = scene.clone();
    let mut arena = RasterImageResourceArena::new();
    let handle = arena.intern_rgba8(1, 1, [17, 73, 191, 128]).unwrap();
    let paint = SemanticPaint::Image(Arc::new(ImagePaint::new(
        handle,
        ImageMapping::default(),
        RasterImageSampling::Linear,
    )));
    let replacement = TrackDefinition {
        values: TrackValues::Paint {
            transition: Arc::new(
                PaintTransition::new(Some(paint.clone()), 1.0, Some(paint.clone()), 0.25).unwrap(),
            ),
        },
        ..original
    };
    let transaction = ExecutionMutationTransaction::from_mutations([
        ExecutionPatch::SetStyle {
            object,
            style: Style {
                fill: Some(Color::GREEN),
                ..Style::default()
            },
            fill_paint: None,
        },
        ExecutionPatch::ReplaceTrack(replacement.clone()),
    ]);
    for _ in 0..2 {
        assert_eq!(
            scene.preflight_execution_transaction(&transaction),
            Err(CompilePatchError::Resource(
                CompiledResourceError::MissingImage(handle,)
            ))
        );
        assert_eq!(scene, before);
    }
    let mut resources = CompiledResources::default();
    resources.capture_paint_from_arena(&arena, &paint).unwrap();
    scene
        .preflight_execution_transaction_with_resources(&transaction, &resources)
        .unwrap();
    assert_eq!(scene, before, "successful preflight also publishes nothing");
    drop(arena);
    scene.merge_prepared_resources(resources);
    for patch in transaction.mutations() {
        scene.apply_execution_patch(patch).unwrap();
    }
    assert_eq!(
        scene.track(replacement.id).unwrap().values,
        replacement.values
    );
    assert_eq!(scene.objects()[0].base_style.fill, Some(Color::GREEN));
    assert_eq!(
        scene.raster_image_resources().get(handle).unwrap().rgba8(),
        &[17, 73, 191, 128]
    );
}
