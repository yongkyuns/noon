use noon_compile::{lower_semantic_execution, CompiledScene, SemanticExecutionIndex};
use noon_core::{
    Color, GeometryRef, SceneDefinition, SemanticObjectProperty, SemanticObjectState, SemanticPaint,
    SemanticStore, SemanticVec3, StoredGeometry, Style, Transform2D, Vec2,
};
use noon_runtime::SceneInstance;

#[test]
fn canonical_semantic_execution_output_builds_runtime_without_recompiling_authored_scene() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
    let object = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 2.0,
    }));
    store.attach_to_scene(object).unwrap();
    store
        .bind_semantic_signal(signal, object, SemanticObjectProperty::ObjectOpacity)
        .unwrap();

    let mut index = SemanticExecutionIndex::new();
    let lowered = lower_semantic_execution(&store, &mut index).unwrap();
    let execution_object = index.execution_object_id(object).unwrap();
    let execution_signal = lowered.reactive().execution_signal_id(signal).unwrap();

    let mut instance = SceneInstance::from_semantic_execution(lowered);
    assert_eq!(instance.frame().objects.len(), 1);
    assert_eq!(instance.frame().objects[0].id, execution_object);
    assert_eq!(instance.frame().objects[0].style.opacity, 0.4);

    instance.take_frame_changes();
    instance
        .set_reactive_input(execution_signal, 0.7_f32)
        .unwrap();

    assert_eq!(instance.frame().objects[0].style.opacity, 0.7);
    assert_eq!(instance.take_frame_changes().object_indices(), &[0]);
}

#[test]
fn representative_legacy_and_semantic_authoring_lower_to_equivalent_runtime_observables() {
    let mut legacy_scene = SceneDefinition::new();
    let legacy_circle = legacy_scene.add(GeometryRef::circle(2.0));
    {
        let object = legacy_scene.object_mut(legacy_circle).unwrap();
        object.transform = Transform2D {
            translation: Vec2::new(4.5, -3.25),
            rotation: 0.75,
            scale: Vec2::new(2.0, 0.5),
        };
        object.style = Style {
            fill: Some(Color::rgba(0.2, 0.4, 0.6, 0.25)),
            stroke: Some(Color::rgba(0.8, 0.1, 0.3, 0.5)),
            stroke_width: 3.5,
            opacity: 0.6,
            ..Style::default()
        };
    }
    let legacy_rectangle = legacy_scene.add(GeometryRef::rectangle(3.0, 1.5));
    {
        let object = legacy_scene.object_mut(legacy_rectangle).unwrap();
        object.transform.translation = Vec2::new(-1.0, 2.0);
        object.style.stroke_width = 0.0;
    }
    let legacy_instance = SceneInstance::new(CompiledScene::compile(&legacy_scene).unwrap());

    let mut semantic_store = SemanticStore::new();
    let mut semantic_circle =
        SemanticObjectState::new(StoredGeometry::Circle { radius: 2.0 });
    semantic_circle.transform.translation = SemanticVec3::new(4.5, -3.25, 0.0);
    semantic_circle.transform.rotation_z = 0.75;
    semantic_circle.transform.scale = SemanticVec3::new(2.0, 0.5, 1.0);
    semantic_circle.style.fill = Some(SemanticPaint::Solid(Color::rgba(0.2, 0.4, 0.6, 1.0)));
    semantic_circle.style.fill_opacity = 0.25;
    semantic_circle.style.stroke = Some(SemanticPaint::Solid(Color::rgba(0.8, 0.1, 0.3, 1.0)));
    semantic_circle.style.stroke_opacity = 0.5;
    semantic_circle.style.stroke_width = 3.5;
    semantic_circle.style.object_opacity = 0.6;
    let semantic_circle = semantic_store.insert_semantic_object(semantic_circle);
    semantic_store.attach_to_scene(semantic_circle).unwrap();

    let mut semantic_rectangle = SemanticObjectState::new(StoredGeometry::Rectangle {
        size: Vec2::new(3.0, 1.5),
    });
    semantic_rectangle.transform.translation = SemanticVec3::new(-1.0, 2.0, 0.0);
    semantic_rectangle.style.stroke_width = 0.0;
    let semantic_rectangle = semantic_store.insert_semantic_object(semantic_rectangle);
    semantic_store.attach_to_scene(semantic_rectangle).unwrap();

    let mut index = SemanticExecutionIndex::new();
    let lowered = lower_semantic_execution(&semantic_store, &mut index).unwrap();
    let semantic_instance = SceneInstance::from_semantic_execution(lowered);

    assert_eq!(legacy_instance.frame().objects.len(), 2);
    assert_eq!(semantic_instance.frame().objects.len(), 2);
    for (legacy, semantic) in legacy_instance
        .frame()
        .objects
        .iter()
        .zip(&semantic_instance.frame().objects)
    {
        // Identity representations intentionally remain migration-specific; compare
        // only the observable execution state and authored painter order.
        assert_eq!(legacy.geometry, semantic.geometry);
        assert_eq!(legacy.transform, semantic.transform);
        assert_eq!(legacy.style, semantic.style);
        assert_eq!(legacy.appearance, semantic.appearance);
    }
}
