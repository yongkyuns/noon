use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    CompositionTimeMap, GeometryRef, ObjectId, Property, RateFunction, Style, TrackDefinition,
    TrackId, TrackTiming, TrackValues, Transform2D,
};
use noon_runtime::SceneInstance;

fn appearance_scene() -> CompiledScene {
    let mut objects = Vec::new();
    let mut tracks = Vec::new();
    let object = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        object,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    objects[object.get() as usize].base_style.opacity = 0.4;
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object,
        property: Property::Appearance,
        values: TrackValues::Scalar { from: 1.0, to: 0.0 },
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });
    CompiledScene::compile_objects(objects, &tracks).expect("appearance scene compiles")
}

#[test]
fn appearance_is_independent_from_semantic_style_opacity() {
    let mut instance = SceneInstance::new(appearance_scene());
    let frame = instance.seek(1.0).expect("valid time");

    assert_eq!(frame.objects[0].style.opacity, 0.4);
    assert_eq!(frame.objects[0].appearance, 0.5);
}

#[test]
fn appearance_seek_and_rewind_are_deterministic() {
    let compiled = appearance_scene();
    let mut sequential = SceneInstance::new(compiled.clone());
    let mut direct = SceneInstance::new(compiled);

    sequential.advance_to(0.5).expect("valid time");
    sequential.advance_to(1.0).expect("valid time");
    sequential.advance_to(2.0).expect("valid time");
    direct.seek(2.0).expect("valid time");
    assert_eq!(sequential.frame(), direct.frame());
    assert_eq!(direct.frame().objects[0].appearance, 0.0);

    direct.seek(0.5).expect("valid rewind");
    assert_eq!(direct.frame().objects[0].appearance, 0.75);
    assert_eq!(direct.frame().objects[0].style.opacity, 0.4);
}

#[test]
fn appearance_values_are_clamped_to_normalized_visibility() {
    let mut objects = Vec::new();
    let mut tracks = Vec::new();
    let object = ObjectId::new(objects.len() as u64);
    objects.push(CompiledObject::new(
        object,
        GeometryRef::circle(1.0),
        Transform2D::IDENTITY,
        Style::default(),
    ));
    tracks.push(TrackDefinition {
        id: TrackId::new(tracks.len() as u64),
        object,
        property: Property::Appearance,
        values: TrackValues::Scalar {
            from: 2.0,
            to: -1.0,
        },
        timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    });
    let compiled = CompiledScene::compile_objects(objects, &tracks).expect("scene compiles");
    let mut instance = SceneInstance::new(compiled);

    assert_eq!(instance.seek(0.0).unwrap().objects[0].appearance, 1.0);
    assert_eq!(instance.seek(1.0).unwrap().objects[0].appearance, 0.0);
}
