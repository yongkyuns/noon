use std::sync::Arc;

use noon_compile::{lower_semantic_execution, SemanticExecutionIndex};
use noon_core::{
    Color, CompositionTimeMap, FontResourceArena, GeometryRef, GeometryResourceArena, Property,
    RateFunction, Rect, SemanticObjectProperty, SemanticObjectState, SemanticPaint, SemanticStore,
    SemanticVec3, StoredGeometry, StrokeCap, StrokeJoin, Style, TextResource, TextSourceKind,
    TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D, Vec2,
};
use noon_runtime::{frame_object_conservative_bounds, SceneInstance};

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
fn canonical_semantic_text_uses_the_same_runtime_frame() {
    let mut store = SemanticStore::new();
    let bounds = Rect::new(Vec2::new(-1.0, -0.5), Vec2::new(2.0, 1.5));
    let handle = store
        .import_text_resource(
            TextResource {
                source: Arc::from("hello"),
                kind: TextSourceKind::Plain,
                runs: Arc::from([]),
                vector_items: Arc::from([]),
                render_items: Arc::from([]),
                parts: Arc::from([]),
                bounds,
                baseline: 0.0,
                layout_artifact: None,
            },
            &FontResourceArena::new(),
            &GeometryResourceArena::new(),
        )
        .unwrap();
    let text = store.insert_semantic_object(SemanticObjectState::new(handle));
    store.attach_to_scene(text).unwrap();

    let lowered = lower_semantic_execution(&store, &mut SemanticExecutionIndex::new()).unwrap();
    let mut instance = SceneInstance::from_semantic_execution(lowered);
    let frame = instance.seek(3.0).unwrap();

    assert_eq!(frame.text(0), Some(handle));
    assert_eq!(frame.objects[0].geometry(), None);
    assert_eq!(frame.render_geometry(0), None);
    assert_eq!(frame.objects[0].text_bounds, Some(bounds));
}

#[test]
fn canonical_text_and_geometry_share_timeline_updates_and_spatial_bounds() {
    let mut store = SemanticStore::new();
    let bounds = Rect::new(Vec2::new(-1.0, -0.5), Vec2::new(2.0, 1.5));
    let handle = store
        .import_text_resource(
            TextResource {
                source: Arc::from("hello"),
                kind: TextSourceKind::Plain,
                runs: Arc::from([]),
                vector_items: Arc::from([]),
                render_items: Arc::from([]),
                parts: Arc::from([]),
                bounds,
                baseline: 0.0,
                layout_artifact: None,
            },
            &FontResourceArena::new(),
            &GeometryResourceArena::new(),
        )
        .unwrap();
    let text = store.insert_semantic_object(SemanticObjectState::new(handle));
    let circle = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.attach_to_scene(text).unwrap();
    store.attach_to_scene(circle).unwrap();

    let mut index = SemanticExecutionIndex::new();
    let lowered = lower_semantic_execution(&store, &mut index).unwrap();
    let text_id = index.execution_object_id(text).unwrap();
    let circle_id = index.execution_object_id(circle).unwrap();
    let (mut compiled, _) = lowered.into_parts();
    for (track, object, destination) in [
        (TrackId::new(0), text_id, Vec2::new(4.0, 2.0)),
        (TrackId::new(1), circle_id, Vec2::new(-2.0, 6.0)),
    ] {
        compiled
            .apply_execution_patch(&noon_compile::ExecutionPatch::AddTrack(TrackDefinition {
                id: track,
                object,
                property: Property::Position,
                values: TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: destination,
                },
                timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            }))
            .unwrap();
    }

    let mut instance = SceneInstance::new(compiled);
    let frame = instance.seek(1.0).unwrap();
    let text_index = frame
        .objects
        .iter()
        .position(|object| object.id == text_id)
        .unwrap();
    let circle_index = frame
        .objects
        .iter()
        .position(|object| object.id == circle_id)
        .unwrap();
    assert_eq!(
        frame.objects[text_index].transform.translation,
        Vec2::new(2.0, 1.0)
    );
    assert_eq!(
        frame.objects[circle_index].transform.translation,
        Vec2::new(-1.0, 3.0)
    );
    assert_eq!(frame.text(text_index), Some(handle));
    assert!(frame.render_geometry(circle_index).is_some());
    assert_eq!(
        frame_object_conservative_bounds(frame, text_index),
        Some(Rect::new(Vec2::new(1.0, 0.5), Vec2::new(4.0, 2.5)))
    );
}

#[test]
fn canonical_lowering_preserves_painter_order_transform_and_paint() {
    let mut semantic_store = SemanticStore::new();
    let mut semantic_circle = SemanticObjectState::new(StoredGeometry::Circle { radius: 2.0 });
    semantic_circle.transform.translation = SemanticVec3::new(4.5, -3.25, 0.0);
    semantic_circle.transform.rotation_z = 0.75;
    semantic_circle.transform.scale = SemanticVec3::new(2.0, 0.5, 1.0);
    semantic_circle.style.fill = Some(SemanticPaint::Solid(Color::rgba(0.2, 0.4, 0.6, 1.0)));
    semantic_circle.style.fill_opacity = 0.25;
    semantic_circle.style.stroke = Some(SemanticPaint::Solid(Color::rgba(0.8, 0.1, 0.3, 1.0)));
    semantic_circle.style.stroke_opacity = 0.5;
    semantic_circle.style.stroke_width = 3.5;
    semantic_circle.style.stroke_join = StrokeJoin::Bevel;
    semantic_circle.style.stroke_cap = StrokeCap::Square;
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

    let objects = &semantic_instance.frame().objects;
    assert_eq!(objects.len(), 2);
    assert_eq!(
        objects[0].id,
        index.execution_object_id(semantic_circle).unwrap()
    );
    assert_eq!(
        objects[1].id,
        index.execution_object_id(semantic_rectangle).unwrap()
    );
    assert_eq!(objects[0].geometry(), Some(&GeometryRef::circle(2.0)));
    assert_eq!(
        objects[1].geometry(),
        Some(&GeometryRef::rectangle(3.0, 1.5))
    );
    assert_eq!(
        objects[0].transform,
        Transform2D {
            translation: Vec2::new(4.5, -3.25),
            rotation: 0.75,
            scale: Vec2::new(2.0, 0.5),
        }
    );
    assert_eq!(
        objects[0].style,
        Style {
            fill: Some(Color::rgba(0.2, 0.4, 0.6, 0.25)),
            stroke: Some(Color::rgba(0.8, 0.1, 0.3, 0.5)),
            stroke_width: 3.5,
            stroke_join: StrokeJoin::Bevel,
            stroke_cap: StrokeCap::Square,
            opacity: 0.6,
            ..Style::default()
        }
    );
    assert_eq!(objects[1].transform.translation, Vec2::new(-1.0, 2.0));
    assert_eq!(objects[1].style.stroke_width, 0.0);
}
