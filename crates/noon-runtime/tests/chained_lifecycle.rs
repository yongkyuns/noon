use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    CompositionTimeMap, Easing, GeometryRef, ObjectId, Property, Style, TrackDefinition, TrackId,
    TrackTiming, TrackValues, Transform2D, TransformTrackEndpoint,
};
use noon_runtime::SceneInstance;

fn chained_scene() -> CompiledScene {
    let mut objects = Vec::new();
    let mut tracks = Vec::new();
    let first = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        first,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    let second = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        second,
        GeometryRef::circle(2.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    let third = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        third,
        GeometryRef::circle(3.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));

    let first_snapshot = TransformTrackEndpoint {
        geometry: objects[first.get() as usize].geometry().unwrap().clone(),
        transform: objects[first.get() as usize].base_transform,
        style: objects[first.get() as usize].base_style,
    };
    let second_snapshot = TransformTrackEndpoint {
        geometry: objects[second.get() as usize].geometry().unwrap().clone(),
        transform: objects[second.get() as usize].base_transform,
        style: objects[second.get() as usize].base_style,
    };
    let third_snapshot = TransformTrackEndpoint {
        geometry: objects[third.get() as usize].geometry().unwrap().clone(),
        transform: objects[third.get() as usize].base_transform,
        style: objects[third.get() as usize].base_style,
    };

    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: first,
        property: Property::Transform,
        values: TrackValues::Object {
            from: first_snapshot,
            to: second_snapshot.clone(),
        },
        timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
        time_map: CompositionTimeMap::identity(),
    });
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: first,
        property: Property::Presence,
        values: TrackValues::Bool {
            from: true,
            to: false,
        },
        timing: TrackTiming::instant(1.0),
        time_map: CompositionTimeMap::identity(),
    });
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: second,
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
        object: second,
        property: Property::Transform,
        values: TrackValues::Object {
            from: second_snapshot,
            to: third_snapshot,
        },
        timing: TrackTiming::new(1.0, 1.0, Easing::Linear),
        time_map: CompositionTimeMap::identity(),
    });
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: second,
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
        object: third,
        property: Property::Presence,
        values: TrackValues::Bool {
            from: false,
            to: true,
        },
        timing: TrackTiming::instant(2.0),
        time_map: CompositionTimeMap::identity(),
    });

    CompiledScene::compile_objects(objects, &tracks).expect("chained lifecycle scene must compile")
}

#[test]
fn chained_replacements_have_exact_presence_handoffs() {
    let mut instance = SceneInstance::new(chained_scene());

    let before_first = instance.seek(0.5).expect("valid time");
    assert!(before_first.is_present(0));
    assert!(!before_first.is_present(1));
    assert!(!before_first.is_present(2));

    let first_handoff = instance.seek(1.0).expect("valid time");
    assert!(!first_handoff.is_present(0));
    assert!(first_handoff.is_present(1));
    assert!(!first_handoff.is_present(2));
    assert_eq!(
        first_handoff.objects[1].geometry(),
        Some(&GeometryRef::circle(2.0))
    );

    let middle = instance.seek(1.5).expect("valid time");
    assert!(!middle.is_present(0));
    assert!(middle.is_present(1));
    assert!(!middle.is_present(2));
    assert_eq!(
        middle.objects[1].geometry(),
        Some(&GeometryRef::circle(2.5))
    );

    let second_handoff = instance.seek(2.0).expect("valid time");
    assert!(!second_handoff.is_present(0));
    assert!(!second_handoff.is_present(1));
    assert!(second_handoff.is_present(2));
    assert_eq!(
        second_handoff.objects[2].geometry(),
        Some(&GeometryRef::circle(3.0))
    );
}

#[test]
fn chained_replacements_direct_seek_matches_forward_and_rewind() {
    let compiled = chained_scene();
    let mut sequential = SceneInstance::new(compiled.clone());
    let mut direct = SceneInstance::new(compiled);

    for time in [0.25, 0.5, 1.0, 1.5, 2.0] {
        sequential.advance_to(time).expect("valid forward time");
    }
    direct.seek(2.0).expect("valid direct seek");
    assert_eq!(sequential.frame(), direct.frame());

    direct.seek(1.0).expect("valid rewind");
    assert!(!direct.frame().is_present(0));
    assert!(direct.frame().is_present(1));
    assert!(!direct.frame().is_present(2));

    direct.seek(0.5).expect("valid rewind before first handoff");
    assert!(direct.frame().is_present(0));
    assert!(!direct.frame().is_present(1));
    assert!(!direct.frame().is_present(2));
}
