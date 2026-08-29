use noon_compile::{CompilePatchError, CompiledScene};
use noon_core::{
    GeometryRef, MutationTransaction, ObjectDefinition, ObjectId, ObjectStateField,
    SceneDefinition, ScenePatch, Style, Transform2D, Vec2, VectorPath,
};

#[test]
fn compiled_property_patches_reject_non_finite_state_without_mutation() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let mut compiled = CompiledScene::compile(&scene).expect("valid scene must compile");

    let cases = [
        (
            ScenePatch::SetGeometry {
                object,
                geometry: GeometryRef::path(
                    VectorPath::new()
                        .move_to(Vec2::ZERO)
                        .line_to(Vec2::new(f32::NAN, 1.0)),
                ),
            },
            ObjectStateField::Geometry,
        ),
        (
            ScenePatch::SetTransform {
                object,
                transform: Transform2D {
                    translation: Vec2::new(f32::NAN, 0.0),
                    ..Transform2D::IDENTITY
                },
            },
            ObjectStateField::Transform,
        ),
        (
            ScenePatch::SetStyle {
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
            compiled.apply_patch(&patch),
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
    let scene = SceneDefinition::new();
    let mut compiled = CompiledScene::compile(&scene).expect("empty scene must compile");
    let before = compiled.clone();
    let object = ObjectId::new(7);
    let mut invalid = ObjectDefinition::new(object, GeometryRef::circle(1.0));
    invalid.transform = Transform2D {
        rotation: f32::NAN,
        ..Transform2D::IDENTITY
    };

    assert_eq!(
        compiled.apply_patch(&ScenePatch::CreateObject(invalid)),
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
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let compiled = CompiledScene::compile(&scene).expect("valid scene must compile");
    let before = compiled.clone();
    let transaction = MutationTransaction::from_mutations([
        ScenePatch::SetTransform {
            object,
            transform: Transform2D {
                translation: Vec2::new(3.0, -2.0),
                ..Transform2D::IDENTITY
            },
        },
        ScenePatch::SetStyle {
            object,
            style: Style {
                stroke_width: f32::NAN,
                ..Style::default()
            },
        },
    ]);

    assert_eq!(
        compiled.preflight_transaction(&transaction),
        Err(CompilePatchError::InvalidObjectState {
            object,
            field: ObjectStateField::Style,
        })
    );
    assert_eq!(compiled, before);
}
