use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    CompositionTimeMap, GeometryRef, ObjectId, Property, RateFunction, Style, TrackDefinition,
    TrackId, TrackTiming, TrackValues, Transform2D, TransformTrackEndpoint, Vec2,
};
use noon_runtime::SceneInstance;

fn matching_scene() -> CompiledScene {
    let mut objects = Vec::new();
    let mut tracks = Vec::new();
    let source_circle = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        source_circle,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    let source_rectangle = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        source_rectangle,
        GeometryRef::rectangle(2.0, 1.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    let target_circle = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        target_circle,
        GeometryRef::circle(2.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    let target_rectangle = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        target_rectangle,
        GeometryRef::rectangle(4.0, 2.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));

    objects[target_circle.get() as usize]
        .base_transform
        .translation = Vec2::new(3.0, 1.0);
    objects[target_rectangle.get() as usize]
        .base_transform
        .translation = Vec2::new(-2.0, -1.0);

    let source_circle_snapshot = TransformTrackEndpoint {
        geometry: objects[source_circle.get() as usize]
            .geometry()
            .unwrap()
            .clone(),
        transform: objects[source_circle.get() as usize].base_transform,
        style: objects[source_circle.get() as usize].base_style,
    };
    let source_rectangle_snapshot = TransformTrackEndpoint {
        geometry: objects[source_rectangle.get() as usize]
            .geometry()
            .unwrap()
            .clone(),
        transform: objects[source_rectangle.get() as usize].base_transform,
        style: objects[source_rectangle.get() as usize].base_style,
    };
    let target_circle_snapshot = TransformTrackEndpoint {
        geometry: objects[target_circle.get() as usize]
            .geometry()
            .unwrap()
            .clone(),
        transform: objects[target_circle.get() as usize].base_transform,
        style: objects[target_circle.get() as usize].base_style,
    };
    let target_rectangle_snapshot = TransformTrackEndpoint {
        geometry: objects[target_rectangle.get() as usize]
            .geometry()
            .unwrap()
            .clone(),
        transform: objects[target_rectangle.get() as usize].base_transform,
        style: objects[target_rectangle.get() as usize].base_style,
    };

    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: source_circle,
        property: Property::Transform,
        values: TrackValues::Object {
            from: source_circle_snapshot,
            to: target_circle_snapshot,
        },
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: source_rectangle,
        property: Property::Transform,
        values: TrackValues::Object {
            from: source_rectangle_snapshot,
            to: target_rectangle_snapshot,
        },
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });

    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: source_circle,
        property: Property::Presence,
        values: TrackValues::Bool {
            from: true,
            to: false,
        },
        timing: TrackTiming::instant(2.0),
        time_map: CompositionTimeMap::identity(),
    });
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: target_circle,
        property: Property::Presence,
        values: TrackValues::Bool {
            from: false,
            to: true,
        },
        timing: TrackTiming::instant(2.0),
        time_map: CompositionTimeMap::identity(),
    });
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: source_rectangle,
        property: Property::Presence,
        values: TrackValues::Bool {
            from: true,
            to: false,
        },
        timing: TrackTiming::instant(2.0),
        time_map: CompositionTimeMap::identity(),
    });
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: target_rectangle,
        property: Property::Presence,
        values: TrackValues::Bool {
            from: false,
            to: true,
        },
        timing: TrackTiming::instant(2.0),
        time_map: CompositionTimeMap::identity(),
    });

    CompiledScene::compile_objects(objects, &tracks).expect("matching-shape lowering must compile")
}

#[test]
fn simultaneous_matches_keep_sources_until_atomic_handoff() {
    let mut instance = SceneInstance::new(matching_scene());

    let before = instance.seek(0.5).expect("valid time");
    assert!(before.is_present(0));
    assert!(before.is_present(1));
    assert!(!before.is_present(2));
    assert!(!before.is_present(3));

    let middle = instance.seek(1.0).expect("valid time");
    assert!(middle.is_present(0));
    assert!(middle.is_present(1));
    assert!(!middle.is_present(2));
    assert!(!middle.is_present(3));
    assert_eq!(
        middle.objects[0].geometry(),
        Some(&GeometryRef::circle(1.5))
    );
    assert_eq!(
        middle.objects[1].geometry(),
        Some(&GeometryRef::rectangle(3.0, 1.5))
    );
    assert_eq!(middle.objects[0].transform.translation, Vec2::new(1.5, 0.5));
    assert_eq!(
        middle.objects[1].transform.translation,
        Vec2::new(-1.0, -0.5)
    );

    let handoff = instance.seek(2.0).expect("valid time");
    assert!(!handoff.is_present(0));
    assert!(!handoff.is_present(1));
    assert!(handoff.is_present(2));
    assert!(handoff.is_present(3));
    assert_eq!(
        handoff.objects[2].geometry(),
        Some(&GeometryRef::circle(2.0))
    );
    assert_eq!(
        handoff.objects[3].geometry(),
        Some(&GeometryRef::rectangle(4.0, 2.0))
    );
}

#[test]
fn simultaneous_matches_direct_seek_matches_forward_and_rewind() {
    let compiled = matching_scene();
    let mut sequential = SceneInstance::new(compiled.clone());
    let mut direct = SceneInstance::new(compiled);

    for time in [0.25, 0.5, 1.0, 1.5, 2.0] {
        sequential.advance_to(time).expect("valid forward time");
    }
    direct.seek(2.0).expect("valid direct seek");
    assert_eq!(sequential.frame(), direct.frame());

    direct.seek(1.0).expect("valid rewind");
    assert!(direct.frame().is_present(0));
    assert!(direct.frame().is_present(1));
    assert!(!direct.frame().is_present(2));
    assert!(!direct.frame().is_present(3));

    direct.seek(2.0).expect("valid second direct seek");
    assert_eq!(sequential.frame(), direct.frame());
}
