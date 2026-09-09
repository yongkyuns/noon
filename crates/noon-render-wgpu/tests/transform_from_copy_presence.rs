use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    Easing, GeometryRef, ObjectId, Property, Style, TrackDefinition, TrackId, TrackTiming,
    TrackValues, Transform2D, TransformTrackEndpoint, Vec2,
};
use noon_render_wgpu::FramePreparer;
use noon_runtime::SceneInstance;

fn copy_scene() -> SceneInstance {
    let source = ObjectId::new(0);
    let target = ObjectId::new(1);
    let copy = ObjectId::new(2);
    let from = TransformTrackEndpoint {
        geometry: GeometryRef::circle(1.0),
        transform: Transform2D {
            translation: Vec2::new(-2.0, 0.0),
            ..Transform2D::IDENTITY
        },
        style: Style::default(),
    };
    let to = TransformTrackEndpoint {
        geometry: GeometryRef::circle(3.0),
        transform: Transform2D {
            translation: Vec2::new(4.0, -2.0),
            ..Transform2D::IDENTITY
        },
        style: Style::default(),
    };
    let objects = vec![
        CompiledObject::new(source, from.geometry.clone(), from.transform, from.style),
        CompiledObject::new(target, to.geometry.clone(), to.transform, to.style),
        CompiledObject::new(copy, from.geometry.clone(), from.transform, from.style),
    ];
    let tracks = [
        TrackDefinition {
            id: TrackId::new(0),
            object: copy,
            property: Property::Transform,
            values: TrackValues::Object { from, to },
            timing: TrackTiming::new(1.0, 2.0, Easing::Linear),
            time_map: Default::default(),
        },
        TrackDefinition {
            id: TrackId::new(1),
            object: copy,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::instant(1.0),
            time_map: Default::default(),
        },
        TrackDefinition {
            id: TrackId::new(2),
            object: copy,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: true,
                to: false,
            },
            timing: TrackTiming::instant(3.0),
            time_map: Default::default(),
        },
        TrackDefinition {
            id: TrackId::new(3),
            object: target,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::instant(3.0),
            time_map: Default::default(),
        },
    ];
    SceneInstance::new(
        CompiledScene::compile_objects(objects, &tracks).expect("execution data compiles"),
    )
}

fn prepared_ids(instance: &mut SceneInstance, preparer: &mut FramePreparer, time: f64) -> Vec<u64> {
    instance.advance_to(time).expect("valid time");
    let changes = instance.take_frame_changes();
    let prepared = preparer.prepare_incremental(instance.frame(), &changes);
    prepared.circle_ids.iter().map(|id| id.get()).collect()
}

#[test]
fn renderer_tracks_transform_from_copy_visible_instance_phases() {
    let mut instance = copy_scene();
    let mut preparer = FramePreparer::new();

    assert_eq!(prepared_ids(&mut instance, &mut preparer, 0.5), vec![0]);
    assert_eq!(prepared_ids(&mut instance, &mut preparer, 1.0), vec![0, 2]);
    assert_eq!(prepared_ids(&mut instance, &mut preparer, 2.0), vec![0, 2]);
    assert_eq!(prepared_ids(&mut instance, &mut preparer, 3.0), vec![0, 1]);
}
