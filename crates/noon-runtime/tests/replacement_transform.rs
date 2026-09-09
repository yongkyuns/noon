use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    CompositionTimeMap, GeometryRef, ObjectId, Property, RateFunction, Style, TrackDefinition,
    TrackId, TrackTiming, TrackValues, Transform2D, TransformTrackEndpoint, Vec2,
};
use noon_runtime::SceneInstance;

fn replacement_scene() -> (CompiledScene, noon_core::ObjectId, noon_core::ObjectId) {
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
        object: source,
        property: Property::Transform,
        values: TrackValues::Object {
            from: source_snapshot,
            to: target_snapshot,
        },
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object: source,
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
        object: target,
        property: Property::Presence,
        values: TrackValues::Bool {
            from: false,
            to: true,
        },
        timing: TrackTiming::instant(2.0),
        time_map: CompositionTimeMap::identity(),
    });

    (
        CompiledScene::compile_objects(objects, &tracks).expect("replacement scene must compile"),
        source,
        target,
    )
}

#[test]
fn replacement_transform_has_exact_stable_identity_handoff() {
    let (compiled, source, target) = replacement_scene();
    let mut instance = SceneInstance::new(compiled);

    let before = instance.seek(0.0).expect("valid time");
    assert_eq!(before.objects.len(), 2);
    assert_eq!(before.objects[0].id, source);
    assert_eq!(before.objects[1].id, target);
    assert!(before.is_present(0));
    assert!(!before.is_present(1));

    let middle = instance.seek(1.0).expect("valid time");
    assert!(middle.is_present(0));
    assert!(!middle.is_present(1));
    assert_eq!(
        middle.objects[0].geometry(),
        Some(&GeometryRef::circle(2.0)),
        "source identity carries interpolated replacement geometry"
    );
    assert_eq!(
        middle.objects[0].transform.translation,
        Vec2::new(2.0, -1.0)
    );

    let handoff = instance.seek(2.0).expect("valid time");
    assert!(!handoff.is_present(0));
    assert!(handoff.is_present(1));
    assert_eq!(handoff.objects[0].id, source);
    assert_eq!(handoff.objects[1].id, target);
    assert_eq!(
        handoff.objects[1].geometry(),
        Some(&GeometryRef::circle(3.0))
    );
    assert_eq!(
        handoff.objects[1].transform.translation,
        Vec2::new(4.0, -2.0)
    );
}

#[test]
fn replacement_transform_direct_seek_matches_forward_playback_and_rewinds() {
    let (compiled, _, _) = replacement_scene();
    let mut sequential = SceneInstance::new(compiled.clone());
    let mut direct = SceneInstance::new(compiled);

    for time in [0.25, 0.5, 1.0, 1.5, 2.0] {
        sequential.advance_to(time).expect("valid forward time");
    }
    direct.seek(2.0).expect("valid direct time");
    assert_eq!(sequential.frame(), direct.frame());

    direct.seek(0.75).expect("valid rewind");
    assert!(direct.frame().is_present(0));
    assert!(!direct.frame().is_present(1));

    direct.seek(2.0).expect("valid second direct seek");
    assert!(!direct.frame().is_present(0));
    assert!(direct.frame().is_present(1));
    assert_eq!(sequential.frame(), direct.frame());
}
