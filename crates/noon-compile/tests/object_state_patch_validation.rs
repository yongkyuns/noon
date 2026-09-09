use noon_compile::{
    CompilePatchError, CompiledObject, CompiledScene, ExecutionMutationTransaction, ExecutionPatch,
};
use noon_core::{GeometryRef, ObjectId, ObjectStateField, Style, Transform2D, Vec2, VectorPath};

#[test]
fn compiled_property_patches_reject_non_finite_state_without_mutation() {
    let mut source_objects = Vec::new();
    let source_tracks = Vec::new();
    let object = ObjectId::new(source_objects.len() as u64);
    source_objects.push(CompiledObject::new(
        object,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    let mut compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
        .expect("valid scene must compile");

    let cases = [
        (
            ExecutionPatch::SetContent {
                object,
                content: GeometryRef::path(
                    VectorPath::new()
                        .move_to(Vec2::ZERO)
                        .line_to(Vec2::new(f32::NAN, 1.0)),
                )
                .into(),
                text_bounds: None,
            },
            ObjectStateField::Geometry,
        ),
        (
            ExecutionPatch::SetTransform {
                object,
                transform: Transform2D {
                    translation: Vec2::new(f32::NAN, 0.0),
                    ..Transform2D::IDENTITY
                },
            },
            ObjectStateField::Transform,
        ),
        (
            ExecutionPatch::SetStyle {
                object,
                style: Style {
                    opacity: f32::INFINITY,
                    ..Style::default()
                },
            },
            ObjectStateField::Style,
        ),
    ];

    for (patch, field) in cases {
        let before = compiled.clone();
        assert_eq!(
            compiled.apply_execution_patch(&patch),
            Err(CompilePatchError::InvalidObjectState { object, field })
        );
        assert_eq!(
            compiled, before,
            "rejected {field} patch mutated compiled state"
        );
    }
}

#[test]
fn compiled_create_rejects_invalid_object_before_allocating_a_slot() {
    let source_objects = Vec::new();
    let source_tracks = Vec::new();
    let mut compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
        .expect("empty scene must compile");
    let before = compiled.clone();
    let object = ObjectId::new(7);
    let mut invalid = CompiledObject::new(
        object,
        GeometryRef::circle(1.0),
        noon_core::Transform2D::IDENTITY,
        Style::default(),
    );
    invalid.base_transform = Transform2D {
        rotation: f32::NAN,
        ..Transform2D::IDENTITY
    };

    assert_eq!(
        compiled.apply_execution_patch(&ExecutionPatch::CreateObject(invalid)),
        Err(CompilePatchError::InvalidObjectState {
            object,
            field: ObjectStateField::Transform,
        })
    );
    assert_eq!(compiled, before);
    assert_eq!(compiled.objects().len(), 0);
    assert_eq!(compiled.live_object_count(), 0);
    assert_eq!(compiled.object_index(object), None);
}

#[test]
fn compiled_transaction_preflight_rejects_late_invalid_object_state_atomically() {
    let mut source_objects = Vec::new();
    let source_tracks = Vec::new();
    let object = ObjectId::new(source_objects.len() as u64);
    source_objects.push(CompiledObject::new(
        object,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    let compiled = CompiledScene::compile_objects(source_objects, &source_tracks)
        .expect("valid scene must compile");
    let before = compiled.clone();
    let transaction = ExecutionMutationTransaction::from_mutations([
        ExecutionPatch::SetTransform {
            object,
            transform: Transform2D {
                translation: Vec2::new(3.0, -2.0),
                ..Transform2D::IDENTITY
            },
        },
        ExecutionPatch::SetStyle {
            object,
            style: Style {
                stroke_width: f32::NAN,
                ..Style::default()
            },
        },
    ]);

    assert_eq!(
        compiled.preflight_execution_transaction(&transaction),
        Err(CompilePatchError::InvalidObjectState {
            object,
            field: ObjectStateField::Style,
        })
    );
    assert_eq!(compiled, before);
}
