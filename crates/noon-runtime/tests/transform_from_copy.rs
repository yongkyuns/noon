use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    CompositionTimeMap, GeometryRef, ObjectId, Property, RateFunction, Style, TrackDefinition,
    TrackId, TrackTiming, TrackValues, Transform2D, TransformTrackEndpoint, Vec2,
};
use noon_runtime::SceneInstance;

fn copy_scene() -> (CompiledScene, ObjectId, ObjectId, ObjectId) {
    let mut objects = Vec::new();
    let mut tracks = Vec::new();
    let source = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        source,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    let target = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        target,
        GeometryRef::circle(3.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    let copy = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        copy,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));

    objects[source.get() as usize].base_transform = Transform2D {
        translation: Vec2::new(-2.0, 0.0),
        ..Transform2D::IDENTITY
    };
    objects[copy.get() as usize].base_transform = Transform2D {
        translation: Vec2::new(-2.0, 0.0),
        ..Transform2D::IDENTITY
    };
    objects[target.get() as usize].base_transform = Transform2D {
        translation: Vec2::new(4.0, -2.0),
        ..Transform2D::IDENTITY
    };

    let source_snapshot = TransformTrackEndpoint {
        geometry: objects[source.get() as usize].geometry().unwrap().clone(),
        transform: objects[source.get() as usize].base_transform,
        style: objects[source.get() as usize].base_style,
    };
    let target_snapshot = TransformTrackEndpoint {
        geometry: objects[target.get() as usize].geometry().unwrap().clone(),
        transform: objects[target.get() as usize].base_transform,
        style: objects[target.get() as usize].base_style,
    };
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: copy,
        property: Property::Transform,
        values: TrackValues::Object {
            from: source_snapshot,
            to: target_snapshot,
        },
        timing: TrackTiming::new(1.0, 2.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: copy,
        property: Property::Presence,
        values: TrackValues::Bool {
            from: false,
            to: true,
        },
        timing: TrackTiming::instant(1.0),
        time_map: CompositionTimeMap::identity(),
    });
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: copy,
        property: Property::Presence,
        values: TrackValues::Bool {
            from: true,
            to: false,
        },
        timing: TrackTiming::instant(3.0),
        time_map: CompositionTimeMap::identity(),
    });
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: target,
        property: Property::Presence,
        values: TrackValues::Bool {
            from: false,
            to: true,
        },
        timing: TrackTiming::instant(3.0),
        time_map: CompositionTimeMap::identity(),
    });

    (
        CompiledScene::compile_objects(objects, &tracks).expect("copy scene must compile"),
        source,
        target,
        copy,
    )
}

#[test]
fn transform_from_copy_has_exact_presence_phases() {
    let (compiled, source, target, copy) = copy_scene();
    let mut instance = SceneInstance::new(compiled);

    let before = instance.seek(0.5).expect("valid time");
    assert_eq!(before.objects[0].id, source);
    assert_eq!(before.objects[1].id, target);
    assert_eq!(before.objects[2].id, copy);
    assert!(before.is_present(0));
    assert!(!before.is_present(1));
    assert!(!before.is_present(2));

    let start = instance.seek(1.0).expect("valid time");
    assert!(start.is_present(0));
    assert!(!start.is_present(1));
    assert!(start.is_present(2));
    assert_eq!(start.objects[2].geometry(), Some(&GeometryRef::circle(1.0)));
    assert_eq!(start.objects[2].transform.translation, Vec2::new(-2.0, 0.0));

    let middle = instance.seek(2.0).expect("valid time");
    assert!(middle.is_present(0));
    assert!(!middle.is_present(1));
    assert!(middle.is_present(2));
    assert_eq!(
        middle.objects[2].geometry(),
        Some(&GeometryRef::circle(2.0))
    );
    assert_eq!(
        middle.objects[2].transform.translation,
        Vec2::new(1.0, -1.0)
    );

    let end = instance.seek(3.0).expect("valid time");
    assert!(end.is_present(0));
    assert!(end.is_present(1));
    assert!(!end.is_present(2));
    assert_eq!(end.objects[1].geometry(), Some(&GeometryRef::circle(3.0)));
    assert_eq!(end.objects[1].transform.translation, Vec2::new(4.0, -2.0));
}

#[test]
fn transform_from_copy_direct_seek_matches_forward_playback_and_rewind() {
    let (compiled, _, _, _) = copy_scene();
    let mut sequential = SceneInstance::new(compiled.clone());
    let mut direct = SceneInstance::new(compiled);

    for time in [0.5, 1.0, 1.5, 2.0, 2.5, 3.0] {
        sequential.advance_to(time).expect("valid forward time");
    }
    direct.seek(3.0).expect("valid direct time");
    assert_eq!(sequential.frame(), direct.frame());

    direct.seek(2.0).expect("valid rewind");
    assert!(direct.frame().is_present(0));
    assert!(!direct.frame().is_present(1));
    assert!(direct.frame().is_present(2));

    direct.seek(0.5).expect("valid pre-start rewind");
    assert!(direct.frame().is_present(0));
    assert!(!direct.frame().is_present(1));
    assert!(!direct.frame().is_present(2));
}
